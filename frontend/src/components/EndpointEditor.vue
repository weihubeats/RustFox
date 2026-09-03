<script setup lang="ts">
/**
 * EndpointEditor：接口编辑器（草稿模式）。
 *
 * - 直接编辑 store 草稿对象（Map 值经 Vue 集合响应式代理，嵌套修改即跟踪）；
 * - Base URL 为本地临时值（不落库），发送时与 path 拼接；
 * - 配置区为横向 Tab 系统：Params / Auth / Headers / Body / Scripts / Tests / Code，
 *   各渲染独立面板组件（Tests 断言、Code 生成代码已从底部工具区迁入）；
 * - Ctrl+S 保存 / Ctrl+Enter 发送；响应区展示状态码、耗时与正文（JSON 自动美化）。
 */
import { computed, defineAsyncComponent, nextTick, onUnmounted, ref, watch } from 'vue'
import { useWorkspaceStore } from '../stores/workspace'
import { useToast } from '../composables/useToast'
import { useFoxApi } from '../composables/useFoxApi'
import { useShortcuts } from '../composables/useShortcuts'
import {
  envBaseUrl,
  environmentVariableMap,
  moduleBaseUrl,
  resolveRequestUrl,
  resolveVariables,
  variableListToMap,
} from '../utils/environment'
import {
  applyMethodDefaults,
  envBadgeLabel as envBadgeLabelOf,
  envBadgeTooltip as envBadgeTooltipOf,
  methodNeedsBody,
} from '../utils/requestBar'
import AuthPanel from './AuthPanel.vue'
import BodyPanel from './BodyPanel.vue'
import CodeExportMenu from './CodeExportMenu.vue'
import CodePanel from './CodePanel.vue'
import DesignPanel from './DesignPanel.vue'
import EnvironmentManager from './EnvironmentManager.vue'
import HeadersPanel from './HeadersPanel.vue'
import MockPanel from './MockPanel.vue'
import MockRuleDialog from './MockRuleDialog.vue'
import CustomSelect from './ui/CustomSelect.vue'
import EmptyState from './ui/EmptyState.vue'
import Icon from './ui/Icon.vue'
import IconButton from './ui/IconButton.vue'
import Menu, { type MenuItem } from './ui/Menu.vue'
import Modal from './ui/Modal.vue'
import ParamsPanel from './ParamsPanel.vue'
import Popconfirm from './ui/Popconfirm.vue'
import ResponsePanel from './ResponsePanel.vue'
import RequestExamplesPanel from './RequestExamplesPanel.vue'
import Tabs from './ui/Tabs.vue'
import TestCaseModal from './TestCaseModal.vue'
import Tooltip from './ui/Tooltip.vue'
import ToolsDrawer from './ToolsDrawer.vue'
import type { TabItem } from './ui/Tabs.vue'
import type {
  ExecuteResponse,
  HttpMethod,
  RequestSpec,
  ResponseExample,
  TestCaseCategory,
} from '../types/foxApi'

const store = useWorkspaceStore()
const toast = useToast()
const api = useFoxApi()

// DocsPanel / TestCasesPanel 内部链路引入 CodeMirror 全家桶（约 300KB），
// 异步化后拆出主 chunk，仅首次切到对应视图时加载
const DocsPanel = defineAsyncComponent(() => import('./DocsPanel.vue'))
const TestCasesPanel = defineAsyncComponent(() => import('./TestCasesPanel.vue'))

const sending = ref(false)
/** 在途请求的取消标识（非空表示有请求可取消）。 */
const activeRequestId = ref<string | null>(null)
const response = ref<ExecuteResponse | null>(null)
const sendError = ref<string | null>(null)

const draft = computed(() => store.activeEndpoint)

const METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS']
const METHOD_OPTIONS = METHODS.map((m) => ({ value: m, label: m }))

// ---------- 配置 Tab 系统 ----------
type ConfigTabKey =
  | 'params'
  | 'auth'
  | 'headers'
  | 'body'
  | 'examples'
  | 'code'

/** 合法 Tab 集合（历史数据可能持久化过已下线的 scripts / tests）。 */
const VALID_TABS: readonly ConfigTabKey[] = ['params', 'auth', 'headers', 'body', 'examples', 'code']

/** 未保存 active_tab 时的智能默认（不写回草稿，避免标记脏）。 */
const smartTab = ref<ConfigTabKey>('params')

/**
 * 配置 Tab：优先读接口保存的 active_tab；未设置（或为已下线 Tab）时按
 * Method 智能默认。用户点击 / 切换 Method 时写回 request.active_tab。
 */
const activeTab = computed<ConfigTabKey>({
  get: () => {
    const saved = draft.value?.request.active_tab as ConfigTabKey | null
    return saved && VALID_TABS.includes(saved) ? saved : smartTab.value
  },
  set: (tab: ConfigTabKey) => {
    smartTab.value = tab
    if (draft.value) draft.value.request.active_tab = tab
  },
})

const BODY_TAB_LABELS: Record<string, string> = {
  json: 'JSON',
  text: 'Text',
  urlencoded: 'x-www-form-urlencoded',
  multipart: 'form-data',
  graphql: 'GraphQL',
  binary: 'Binary',
}

const configTabs = computed<TabItem[]>(() => {
  const d = draft.value
  if (!d) return []
  const bodyMode = d.request.body.mode
  return [
    { key: 'params', label: 'Params', count: d.request.params.length },
    { key: 'auth', label: 'Auth' },
    { key: 'headers', label: 'Headers', count: d.request.headers.length },
    {
      key: 'body',
      label: bodyMode !== 'none' ? `Body (${BODY_TAB_LABELS[bodyMode] ?? bodyMode})` : 'Body',
      count: bodyMode !== 'none' ? 1 : undefined,
    },
    { key: 'examples', label: 'Examples' },
    { key: 'code', label: 'Code' },
  ]
})

/** 接口切换：无保存的 active_tab 时按 Method 设定智能默认（不落库）。 */
watch(
  () => draft.value?.id,
  () => {
    smartTab.value = draft.value && methodNeedsBody(draft.value.method) ? 'body' : 'params'
    // 旧大 body 不再常驻：单例 response 切 Tab 不清空原来既占内存又可能错显。
    response.value = null
    sendError.value = null
  },
  { immediate: true },
)

/**
 * 手动切换 Method：POST 系 → Body（空体初始化 `{}` + application/json，有体则保持）；其余 → Params。
 * 仅在【同一接口内】method 变化时应用：打开 / 切换接口时 draft 从 null → 有值
 * 或 id 变化，method 必然「变」一次——那是一次载入而非用户编辑，回写
 * applyMethodDefaults 的副作用（body 初始化 / Content-Type / active_tab）
 * 会让刚打开的接口立刻被判定为「有改动」（isDirty 误报）。
 *
 * 往返还原：首次切换时记录切换前的 request 快照；切回记录的原方法时
 * 整体还原快照——否则 applyMethodDefaults 的副作用不可逆，用户
 * 「GET→POST→GET」后草稿仍与保存态不同，持续显示「有改动」。
 */
let methodRevert: { from: string; snapshot: RequestSpec } | null = null

watch(
  () => [draft.value?.id, draft.value?.method] as const,
  ([id, m], prev) => {
    if (!m || !draft.value) return
    if (id !== prev?.[0]) {
      methodRevert = null
      return
    }
    const d = draft.value
    if (methodRevert && m === methodRevert.from) {
      // 切回上次的方法：整体还原切换前的 request（撤销默认初始化副作用）
      d.request = methodRevert.snapshot
      const restored = d.request.active_tab
      smartTab.value = (restored && VALID_TABS.includes(restored as ConfigTabKey)
        ? (restored as ConfigTabKey)
        : methodNeedsBody(m)
          ? 'body'
          : 'params')
      methodRevert = null
      return
    }
    methodRevert ??= { from: prev?.[1] ?? m, snapshot: JSON.parse(JSON.stringify(d.request)) as RequestSpec }
    const tab = applyMethodDefaults(d.request, m)
    d.request.active_tab = tab
    smartTab.value = tab
  },
)

function prettyBody(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2)
  } catch {
    return raw
  }
}

function isAbsolutePath(p: string): boolean {
  return p.startsWith('http://') || p.startsWith('https://')
}

/** 地址栏展示前缀（唯一真实数据源）：环境 base_url 变量 > 会话 Base URL。 */
const urlDomain = computed(() => (draft.value ? store.urlDomain : ''))

/** 面包屑：接口所属文件夹名。 */
const folderName = computed(() => {
  if (!draft.value?.folder_id) return ''
  return store.folders.find((f) => f.id === draft.value!.folder_id)?.name ?? ''
})

/** 路径是否为完整绝对 URL（此时不显示前缀 chip，地址栏直接展示全文）。 */
const isAbsPath = computed(() => (draft.value ? isAbsolutePath(draft.value.path) : false))

/** 激活环境（chip 色点）。 */
const activeEnv = computed(
  () => store.environments.find((e) => e.id === store.activeEnvId) ?? null,
)
const activeEnvName = computed(() => activeEnv.value?.name ?? '')

/**
 * 当前标签页绑定的模块（id，空 = 默认模块）。按 endpointId 持久化到 localStorage，
 * 刷新 / 重开标签后保持「该接口归属哪个服务」的记忆。
 */
const moduleId = ref<string | null>(null)

watch(
  () => draft.value?.id,
  (id) => {
    if (!id) {
      moduleId.value = null
      return
    }
    moduleId.value = localStorage.getItem(`rustfox:module:${id}`)
  },
  { immediate: true },
)

function setModule(id: string): void {
  moduleId.value = id || null
  if (draft.value?.id) {
    if (id) localStorage.setItem(`rustfox:module:${draft.value.id}`, id)
    else localStorage.removeItem(`rustfox:module:${draft.value.id}`)
  }
}

/** 环境变量 + 项目变量 + 全局变量合并表（chips / 预览解析用；优先级 环境 > 项目 > 全局）。 */
const envVars = computed(() => ({
  ...variableListToMap(store.globalVariables),
  ...(store.project?.variables ?? {}),
  ...environmentVariableMap(activeEnv.value, store.project?.id),
}))

/** 地址栏前缀 chip 文案：绑定的模块基址 > 环境 base_url 变量的「解析后」实际值或会话 Base URL。 */
const resolvedDomain = computed(() => {
  if (moduleId.value && activeEnv.value) {
    const b = moduleBaseUrl(activeEnv.value, moduleId.value)
    if (b) return resolveVariables(b, envVars.value)
  }
  const src = urlDomain.value
  if (!src) return ''
  return resolveVariables(src, envVars.value)
})

/** chip 变量引用未解析（环境未定义该变量）。 */
const urlUnresolved = computed(
  () => urlDomain.value.startsWith('{{') && resolvedDomain.value === urlDomain.value,
)

/** 基础 URL 标签样式：环境变量已解析 → 主题色；未解析 → 警告；会话回退 → 中性。 */
const chipClass = computed(() => {
  if (urlUnresolved.value) return 'warn'
  if (urlDomain.value.startsWith('{{')) return 'env'
  return 'session'
})

/** 点击基础 URL 标签 → 打开环境管理。 */
const showEnvManager = ref(false)
const showMockRules = ref(false)

/** Base URL 紧凑标签：直接展示解析后的裸域名（无域名时退回环境名），一眼可见实际发送目标。 */
const envBadgeLabel = computed(() =>
  envBadgeLabelOf({
    urlDomain: urlDomain.value,
    resolvedDomain: resolvedDomain.value,
    envName: activeEnvName.value,
  }),
)

/** Base URL 标签悬浮提示：`环境：X | 基础路径：https://...`（无环境时仅展示路径来源）。 */
const envBadgeTooltip = computed(() => {
  if (!draft.value || isAbsPath.value) return ''
  return envBadgeTooltipOf({
    urlDomain: urlDomain.value,
    resolvedDomain: resolvedDomain.value,
    envName: activeEnvName.value,
  })
})

/** 路径输入框元素引用（快捷按钮聚焦回跳）。 */
const urlInputEl = ref<HTMLInputElement | null>(null)

/** 路径输入框 placeholder：有基础 URL 时提示自动拼接，无则提示粘贴完整 URL。 */
const urlPlaceholder = computed(() => {
  const base = '输入接口路径，如 /api/v1/users'
  if (!urlDomain.value) return `${base}，或直接粘贴完整 URL`
  return `${base}，自动拼接 ${resolvedDomain.value || urlDomain.value}`
})

/** 路径输入框（与 chip 组成完整请求地址）；粘贴完整 URL 时自动拆分。 */
const urlPath = computed({
  get: () => {
    const d = draft.value
    if (!d) return ''
    return d.path
  },
  set: (value: string) => {
    const d = draft.value
    if (!d) return
    const v = value.trim()
    if (!v) return

    // 1) 粘贴/改写完整 URL：origin 写入域名源（环境变量优先），query 并入参数。
    const abs = v.match(/^(?:https?|wss?):\/\/[^/]+/)
    if (abs) {
      let rest = v.slice(abs[0].length) || '/'
      const qIdx = rest.indexOf('?')
      if (qIdx !== -1) {
        const qs = rest.slice(qIdx + 1)
        rest = rest.slice(0, qIdx) || '/'
        for (const [key, val] of new URLSearchParams(qs).entries()) {
          d.request.params.push({ key, value: val, enabled: true, description: '' })
        }
      }
      if (store.urlDomain.startsWith('{{')) {
        void store.setEnvironmentBaseUrl(abs[0])
      } else {
        store.sessionBaseUrl = abs[0]
      }
      d.path = rest.startsWith('/') ? rest : `/${rest}`
      return
    }

    // 2) 以 `{{变量}}` 开头：变量引用成为域名源。
    const varRef = v.match(/^\{\{[^{}]+\}\}/)
    if (varRef) {
      store.sessionBaseUrl = varRef[0]
      d.path = v.slice(varRef[0].length) || '/'
      return
    }

    // 3) 其余视为路径本身。
    d.path = v.startsWith('/') ? v : `/${v}`
  },
})

/** 请求地址（与 send / 代码生成 / 压测共用）；多模块按绑定模块基址拼接，变量由 resolveRequestUrl 解析。 */
function buildUrl(): string {
  const d = draft.value
  if (!d) return ''
  if (isAbsolutePath(d.path)) return d.path
  // 有环境（默认模块或显式绑定模块基址）时按多模块规则解析。
  if (activeEnv.value && (moduleId.value || envBaseUrl(activeEnv.value))) {
    return resolveRequestUrl(activeEnv.value, moduleId.value, d.path, envVars.value, d.project_id).url
  }
  const path = d.path.startsWith('/') ? d.path : `/${d.path}`
  return `${store.urlDomain}${path}`
}

async function send(): Promise<void> {
  if (!draft.value || sending.value) return
  sending.value = true
  sendError.value = null
  const url = buildUrl()
  const rid = crypto.randomUUID()
  activeRequestId.value = rid
  try {
    response.value = await store.send(draft.value, url, rid)
    // 历史已迁至侧栏「请求历史」页签，发送后由 store 统一刷新。
    void store.loadHistories()
  } catch (err) {
    const e = err as Error & { code?: string }
    if (e?.code === 'CANCELLED') {
      // 用户主动取消：不视为错误，保留上一次结果。
      toast.info('请求已取消')
      sendError.value = null
    } else {
      sendError.value = err instanceof Error ? err.message : String(err)
      response.value = null
    }
  } finally {
    if (activeRequestId.value === rid) activeRequestId.value = null
    sending.value = false
  }
}

/** 取消在途请求（后端中止连接，命令随即以 CANCELLED 返回）。 */
function cancelSend(): void {
  if (!activeRequestId.value) return
  void api.cancelRequest(activeRequestId.value)
  toast.info('正在取消请求…')
}

/** 保存：名称为空或仍是默认「未命名接口」时，先弹「名称 + 保存位置」确认框，确认后再落库。 */
const showNameDialog = ref(false)
const pendingName = ref('')
const pendingFolderId = ref('')

/** 保存位置（文件夹）选项：树形展平，子目录按层级缩进展示。 */
interface FolderOption {
  value: string
  label: string
  depth: number
}
const folderOptions = computed<FolderOption[]>(() => {
  const out: FolderOption[] = []
  const walk = (parentId: string | null, depth: number): void => {
    store.folders
      .filter((f) => f.parent_id === parentId)
      .sort((a, b) => a.sort_order - b.sort_order)
      .forEach((f) => {
        out.push({ value: f.id, label: f.name, depth })
        walk(f.id, depth + 1)
      })
  }
  walk(null, 0)
  return out
})

async function save(): Promise<void> {
  if (!draft.value) return
  const name = draft.value.name.trim()
  if (!name || name === '未命名接口') {
    pendingName.value = ''
    pendingFolderId.value = draft.value.folder_id ?? ''
    showNameDialog.value = true
    return
  }
  await store.saveActiveDraft()
}

async function confirmName(): Promise<void> {
  if (!draft.value) return
  const name = pendingName.value.trim()
  if (!name) {
    toast.warning('接口名称不能为空')
    return
  }
  draft.value.name = name
  draft.value.folder_id = pendingFolderId.value || null
  showNameDialog.value = false
  await store.saveActiveDraft()
}

// ---------- 二级导航 + 保存为用例 ----------
const SUB_NAV: { key: 'debug' | 'design' | 'docs' | 'cases' | 'mock'; label: string }[] = [
  { key: 'debug', label: '调试' },
  { key: 'design', label: '设计' },
  { key: 'docs', label: '文档预览' },
  { key: 'cases', label: '测试用例' },
  { key: 'mock', label: 'Mock' },
]

/** 切换接口时回到「调试」页。 */
watch(
  () => draft.value?.id,
  () => store.setActiveView('debug'),
  { immediate: true },
)

const saveMenuEl = ref<InstanceType<typeof Menu> | null>(null)

function openSaveMenu(event: MouseEvent): void {
  saveMenuEl.value?.openAt(event.currentTarget as HTMLElement, [
    { key: 'save-case', label: '保存为用例', icon: 'list' },
  ])
}

function onSaveMenuSelect(item: MenuItem): void {
  if (item.key === 'save-case') openSaveCaseModal()
}

const showTestCaseModal = ref(false)
const pendingCaseName = ref('')

/** 保存为用例：提取当前请求快照（URL / Params / Headers / Body）存入 test_cases。 */
function openSaveCaseModal(): void {
  const d = draft.value
  if (!d) return
  pendingCaseName.value = d.name !== '未命名接口' ? `${d.method} ${d.path}` : ''
  showTestCaseModal.value = true
}

async function onSaveCaseSubmit(payload: {
  name: string
  category: TestCaseCategory
}): Promise<void> {
  const d = draft.value
  if (!d) return
  await store.saveTestCase(d.id, payload.name, payload.category, d.request, d.path, d.method)
}

// ---------- 请求区 / 响应区高度分割（Splitter） ----------
const REQUEST_MIN = 80
const REQUEST_DEFAULT = 200
/** 请求区最大高度 = 编辑器高度 - MAX_OFFSET，保证响应区至少留 MAX_OFFSET px。 */
const MAX_OFFSET = 100

const editorEl = ref<HTMLElement | null>(null)
const requestBodyHeight = ref(REQUEST_DEFAULT)
const splitterDragging = ref(false)
const requestBodyCollapsed = computed(() => requestBodyHeight.value <= REQUEST_MIN)

/** 尚未产生响应（也没发送失败）时：请求区占满剩余高度，隐藏分割条与响应占位。 */
const hasResponse = computed(() => !!response.value || !!sendError.value)

let splitStartY = 0
let splitStartHeight = 0
let splitDragging = false

function requestMaxHeight(): number {
  return Math.max((editorEl.value?.clientHeight ?? 600) - MAX_OFFSET, REQUEST_MIN + 40)
}

/** 分割条 mousedown：开始拖拽，动态调整请求区高度（响应区 flex:1 自动补位）。 */
function onSplitterDown(event: MouseEvent): void {
  if (event.button !== 0) return
  event.preventDefault()
  splitDragging = true
  splitterDragging.value = true
  splitStartY = event.clientY
  splitStartHeight = requestBodyHeight.value
  document.body.style.userSelect = 'none'
  document.addEventListener('mousemove', onSplitterMove)
  document.addEventListener('mouseup', onSplitterUp)
}

function onSplitterMove(event: MouseEvent): void {
  if (!splitDragging) return
  const next = splitStartHeight + (event.clientY - splitStartY)
  requestBodyHeight.value = Math.min(Math.max(next, REQUEST_MIN), requestMaxHeight())
}

function onSplitterUp(): void {
  if (!splitDragging) return
  splitDragging = false
  splitterDragging.value = false
  document.body.style.userSelect = ''
  document.removeEventListener('mousemove', onSplitterMove)
  document.removeEventListener('mouseup', onSplitterUp)
}

/** 双击分割条 / 点击微调按钮：请求区收缩到最小高度，再点恢复默认高度。 */
function toggleRequestBody(): void {
  requestBodyHeight.value = requestBodyCollapsed.value ? REQUEST_DEFAULT : REQUEST_MIN
}

// ---------- 响应示例 ----------
const viewingExample = ref<ResponseExample | null>(null)
const activeExamples = computed(() => store.examples.get(draft.value?.id ?? '') ?? [])

const showExampleDialog = ref(false)
const exampleName = ref('')

function saveExample(): void {
  if (!draft.value || !response.value) return
  exampleName.value = `${draft.value.method} ${new Date().toLocaleTimeString('zh-CN')}`
  showExampleDialog.value = true
}

async function confirmSaveExample(): Promise<void> {
  if (!draft.value || !response.value) return
  const name = exampleName.value.trim()
  if (!name) {
    toast.warning('示例名称不能为空')
    return
  }
  try {
    await store.saveAsExample(draft.value.id, name, response.value)
    showExampleDialog.value = false
  } catch (err) {
    toast.error('保存示例失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

function viewExample(ex: ResponseExample): void {
  viewingExample.value = viewingExample.value?.id === ex.id ? null : ex
}

async function removeExample(ex: ResponseExample): Promise<void> {
  if (!draft.value) return
  try {
    await store.removeExample(draft.value.id, ex.id)
    if (viewingExample.value?.id === ex.id) viewingExample.value = null
  } catch (err) {
    toast.error('删除示例失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

// ---------- 工具抽屉（生成代码 / 测试 / 压测） ----------
const showTools = ref(false)

const requestUrl = computed(() => (draft.value ? buildUrl() : ''))

/** 路径输入框 Enter → 发送；Esc → 清空路径（setter 忽略空串，直接写草稿）。 */
function onUrlKeydown(event: KeyboardEvent): void {
  if (event.key === 'Enter') {
    event.preventDefault()
    event.stopPropagation()
    if (!sending.value) void send()
  } else if (event.key === 'Escape') {
    event.preventDefault()
    if (draft.value) draft.value.path = ''
  }
}

/** 快捷按钮：清空路径。 */
function clearPath(): void {
  if (draft.value) draft.value.path = ''
}

/** 快捷按钮：复制完整请求地址。 */
async function copyRequestUrl(): Promise<void> {
  if (!requestUrl.value) return
  await navigator.clipboard.writeText(requestUrl.value)
  toast.info('地址已复制')
}

/**
 * 全局快捷键（集中注册表，见 useShortcuts；帮助面板自动收录）。
 * inInput: true 保持原行为——原来是裸 window 监听，输入框内同样生效。
 */
useShortcuts([
  {
    id: 'editor.save',
    key: 's',
    group: '请求编辑',
    description: '保存当前接口',
    inInput: true,
    handler: () => save(),
  },
  {
    id: 'editor.send',
    key: 'Enter',
    group: '请求编辑',
    description: '发送当前请求',
    inInput: true,
    handler: () => void send(),
  },
  {
    id: 'editor.new-request-t',
    key: 't',
    group: '请求编辑',
    description: '新建接口',
    inInput: true,
    handler: () => store.openNewEndpoint(null),
  },
  {
    id: 'editor.new-request-n',
    key: 'n',
    group: '请求编辑',
    description: '新建接口',
    inInput: true,
    handler: () => store.openNewEndpoint(null),
  },
])

/** 新建接口后自动聚焦地址输入框（TabBar「+」/ ⌘T ⌘N / 树内新建共用），便于直接输入路径。 */
watch(
  () => store.focusTitleSignal,
  () => {
    void nextTick(() => {
      urlInputEl.value?.focus()
    })
  },
)

onUnmounted(() => {
  onSplitterUp()
})
</script>

<template>
  <div ref="editorEl" v-if="draft" class="editor">
    <div class="editor-row breadcrumb-row">
      <span class="crumb">
        <span class="crumb-part">{{ store.project?.name ?? '未命名项目' }}</span>
        <template v-if="folderName">
          <span class="crumb-sep">/</span>
          <span class="crumb-part">{{ folderName }}</span>
        </template>
        <span class="crumb-sep">/</span>
        <input
          v-model="draft.name"
          class="crumb-name"
          placeholder="接口名称"
          spellcheck="false"
          title="点击可直接修改接口名称"
        />
      </span>
      <span class="breadcrumb-spacer"></span>
    </div>

    <!-- 二级导航：调试 | 设计 | 文档预览 | 测试用例 (N) | Mock -->
    <div class="sub-nav" role="tablist">
      <button
        v-for="nav in SUB_NAV"
        :key="nav.key"
        class="sub-nav-item"
        :class="{ active: store.activeView === nav.key }"
        type="button"
        role="tab"
        @click="store.setActiveView(nav.key)"
      >
        {{ nav.label }}
        <span v-if="nav.key === 'cases'" class="sub-nav-badge">{{ store.testCaseCount }}</span>
      </button>
    </div>

    <template v-if="store.activeView === 'debug'">
      <div class="editor-row">
      <div class="request-bar">
        <CustomSelect
          class="method-select"
          :model-value="draft.method"
          :options="METHOD_OPTIONS"
          @update:model-value="draft.method = String($event) as HttpMethod"
        >
          <template #display="{ label }">
            <span :class="`m-select-${draft.method.toLowerCase()}`">{{ label }}</span>
          </template>
        </CustomSelect>
        <span class="req-bar-divider"></span>
        <Tooltip v-if="urlDomain && !isAbsPath" :content="envBadgeTooltip" placement="bottom">
          <button
            type="button"
            class="env-badge"
            :class="chipClass"
            @click="showEnvManager = true"
          >
            <Icon name="globe" :size="13" class="env-badge-icon" />
            <span class="env-badge-text">{{ envBadgeLabel }}</span>
            <Icon name="chevron-down" :size="11" class="env-badge-chevron" />
          </button>
        </Tooltip>
        <Tooltip
          v-if="activeEnv && activeEnv.modules.length > 0 && !isAbsPath"
          :content="'选择该请求归属的服务模块；未绑定（默认）时使用默认模块基址'"
          placement="bottom"
        >
          <CustomSelect
            class="mod-select"
            pop-class="mod-pop"
            :model-value="moduleId ?? ''"
            :options="[
              { value: '', label: '默认模块' },
              ...activeEnv.modules.map((m) => ({ value: m.id, label: m.module_name })),
            ]"
            size="sm"
            @change="setModule(String($event))"
          />
        </Tooltip>
        <div class="url-input-wrap">
          <input
            ref="urlInputEl"
            v-model="urlPath"
            class="url-input"
            spellcheck="false"
            :placeholder="urlPlaceholder"
            @keydown="onUrlKeydown"
          />
          <template v-if="urlPath">
            <Tooltip content="复制完整请求地址" placement="top" class="url-qbtn url-qbtn-copy">
              <button type="button" class="url-qbtn-btn" @click="copyRequestUrl">
                <Icon name="copy" :size="13" />
              </button>
            </Tooltip>
            <Tooltip content="清空路径 (Esc)" placement="top" class="url-qbtn">
              <button type="button" class="url-qbtn-btn" @click="clearPath">
                <Icon name="x" :size="13" />
              </button>
            </Tooltip>
          </template>
        </div>
        <button v-if="!sending" class="rf-btn rf-btn-send bar-send" type="button" @click="send">
          <Icon name="send" :size="14" />
          发送
        </button>
        <button
          v-else
          class="rf-btn rf-btn-danger bar-send"
          type="button"
          @click="cancelSend"
        >
          <Icon name="stop" :size="14" /> 取消
        </button>
      </div>
      <div class="editor-actions">
        <button class="rf-btn rf-btn-sm" type="button" title="断言测试 / 压测" @click="showTools = true">
          <Icon name="gauge" :size="13" /> 工具
        </button>
        <CodeExportMenu :draft="draft" :url="requestUrl" />
        <div class="save-group">
          <button class="rf-btn save-main" type="button" @click="save">
            <Icon name="save" :size="14" /> 保存 (⌘S)
          </button>
          <button
            class="rf-btn save-arrow"
            type="button"
            title="更多保存选项"
            @click="openSaveMenu($event)"
          >
            <Icon name="chevron-down" :size="12" />
          </button>
        </div>
      </div>
    </div>

    <div
      class="config-box"
      :class="{ collapsed: requestBodyCollapsed, grow: !hasResponse }"
      :style="hasResponse ? { height: `${requestBodyHeight}px` } : undefined"
    >
      <Tabs v-model="activeTab" :tabs="configTabs" size="sm" />
      <ParamsPanel v-if="activeTab === 'params'" :draft="draft" />
      <AuthPanel v-else-if="activeTab === 'auth'" :draft="draft" />
      <HeadersPanel v-else-if="activeTab === 'headers'" :draft="draft" />
      <BodyPanel v-else-if="activeTab === 'body'" :draft="draft" />
      <RequestExamplesPanel v-else-if="activeTab === 'examples'" :draft="draft" />
      <CodePanel v-else :draft="draft" :url="requestUrl" />
    </div>

    <template v-if="hasResponse">
      <div
        class="rp-splitter"
        :class="{ dragging: splitterDragging }"
        title="拖拽调整请求区高度（双击折叠 / 展开）"
        @mousedown="onSplitterDown"
        @dblclick="toggleRequestBody"
      >
        <button
          class="rp-splitter-btn"
          type="button"
          :title="requestBodyCollapsed ? '展开请求区' : '折叠请求区'"
          @mousedown.stop
          @dblclick.stop
          @click="toggleRequestBody"
        >
          <Icon :name="requestBodyCollapsed ? 'chevron-up' : 'chevron-down'" :size="11" />
        </button>
      </div>

      <div class="response-zone">
        <ResponsePanel v-if="response" :response="response" @save-example="saveExample" />
        <div v-else-if="sendError" class="send-error" role="alert">
          <span>发送失败：{{ sendError }}</span>
        </div>
        <EmptyState
          v-else
          class="response-empty"
          icon="send"
          title="尚未发送请求"
          description="点击发送按钮或按 Cmd + Enter (Ctrl + Enter) 获取响应结果"
        />
      </div>
    </template>
    <p v-else class="response-hint">发送请求后，响应将显示在这里</p>
    <div v-if="activeExamples.length" class="examples">
      <h3 class="section-title">响应示例 ({{ activeExamples.length }})</h3>
      <div v-for="ex in activeExamples" :key="ex.id" class="example-row">
        <button
          class="example-main"
          type="button"
          :class="{ open: viewingExample?.id === ex.id }"
          @click="viewExample(ex)"
        >
          <span class="example-status" :class="{ err: ex.status >= 400 }">{{ ex.status }}</span>
          <span class="example-name">{{ ex.name }}</span>
          <span class="example-meta">{{ ex.created_at.slice(0, 16).replace('T', ' ') }}</span>
        </button>
        <Popconfirm :title="`删除示例「${ex.name}」？`" @confirm="removeExample(ex)">
            <IconButton name="trash" :size="13" tone="danger" title="删除示例" />
          </Popconfirm>
      </div>
      <pre v-if="viewingExample" class="example-body">{{ prettyBody(viewingExample.body) }}</pre>
    </div>
    </template>
    <DesignPanel v-else-if="store.activeView === 'design'" :draft="draft" @save="save" />
    <DocsPanel v-else-if="store.activeView === 'docs'" :draft="draft" :url="requestUrl" />
    <TestCasesPanel v-else-if="store.activeView === 'cases'" :draft="draft" />
    <MockPanel
      v-else-if="store.activeView === 'mock'"
      :draft="draft"
      @open-manager="showMockRules = true"
    />
  </div>
  <div v-else class="editor-empty">
    <p>从左侧选择接口开始编辑</p>
  </div>

  <Modal v-model:open="showNameDialog" title="保存接口" width="420px">
    <p class="name-hint">请为接口填写名称（必填）：</p>
    <input
      v-model="pendingName"
      class="rf-input name-dialog-input"
      placeholder="例如：获取用户列表"
      spellcheck="false"
      @keyup.enter="confirmName"
    />
    <p class="name-hint folder-hint">保存位置（文件夹）：</p>
    <CustomSelect
      class="save-folder-select"
      :model-value="pendingFolderId"
      :options="folderOptions"
      placeholder="根目录（不选择文件夹）"
      @update:model-value="pendingFolderId = String($event)"
    >
      <template #display="{ label }">
        <span class="save-folder-display">{{ label || '根目录（不选择文件夹）' }}</span>
      </template>
      <template #option="{ option }">
        <span :style="{ paddingLeft: `${(option as FolderOption).depth * 16 + 4}px` }">
          {{ option.label }}
        </span>
      </template>
    </CustomSelect>
    <template #footer>
      <button class="rf-btn" type="button" @click="showNameDialog = false">取消</button>
      <button class="rf-btn rf-btn-primary" type="button" @click="confirmName">
        <Icon name="save" :size="14" /> 保存
      </button>
    </template>
  </Modal>

  <Modal v-model:open="showExampleDialog" title="保存响应示例" width="360px">
    <p class="name-hint">请输入示例名称：</p>
    <input
      v-model="exampleName"
      class="rf-input name-dialog-input"
      placeholder="例如：成功响应"
      spellcheck="false"
      @keyup.enter="confirmSaveExample"
    />
    <template #footer>
      <button class="rf-btn" type="button" @click="showExampleDialog = false">取消</button>
      <button class="rf-btn rf-btn-primary" type="button" @click="confirmSaveExample">保存</button>
    </template>
  </Modal>

  <ToolsDrawer :open="showTools" :draft="draft" :url="requestUrl" @close="showTools = false" />

  <EnvironmentManager v-model:open="showEnvManager" />

  <TestCaseModal
    :open="showTestCaseModal"
    title="保存为测试用例"
    :name="pendingCaseName"
    @update:open="showTestCaseModal = $event"
    @submit="onSaveCaseSubmit"
  />

  <MockRuleDialog v-if="showMockRules" @close="showMockRules = false" />

  <Menu ref="saveMenuEl" @select="onSaveMenuSelect" />
</template>

<style scoped>
.editor {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 12px;
  overflow-y: auto;
  height: 100%;
}

.editor-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.sub-nav {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-2);
  align-self: flex-start;
}

.sub-nav-item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 14px;
  border: none;
  border-radius: 6px;
  background: none;
  font-family: inherit;
  font-size: 12.5px;
  color: var(--text-2);
  cursor: pointer;
  transition:
    color var(--dur) var(--ease),
    background var(--dur) var(--ease);
}
.sub-nav-item:hover {
  color: var(--text-1);
  background: var(--bg-hover);
}
.sub-nav-item.active {
  color: var(--text-1);
  background: var(--bg-elevated);
  box-shadow: var(--shadow-sm);
}

.sub-nav-badge {
  min-width: 16px;
  padding: 0 5px;
  border-radius: 999px;
  font-family: var(--font-mono);
  font-size: 10.5px;
  line-height: 16px;
  text-align: center;
  color: var(--accent);
  background: var(--accent-tint, rgba(168, 85, 247, 0.16));
}

.save-group {
  display: flex;
  align-items: stretch;
}
.save-main {
  border-radius: 7px 0 0 7px;
}
.save-arrow {
  border-radius: 0 7px 7px 0;
  border-left: 1px solid rgba(255, 255, 255, 0.18);
  padding: 0 7px;
}

.method-select {
  width: 108px;
  flex-shrink: 0;
  font-weight: 700;
}

.m-select-get { color: var(--rf-success); }
.m-select-post { color: var(--rf-warning); }
.m-select-put { color: var(--rf-info); }
.m-select-delete { color: var(--rf-danger); }
.m-select-patch { color: var(--patch); }
.m-select-head, .m-select-options { color: var(--rf-text-muted); }

/* 统一请求栏：方法下拉 + 基础URL标签 + 路径输入合并为一个控件 */
.request-bar {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: stretch;
  height: var(--h-md);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius);
  background: var(--bg-card);
  overflow: hidden;
  transition:
    border-color var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}
.request-bar:hover {
  border-color: var(--accent);
  box-shadow: 0 0 0 1px var(--accent-tint);
}
.request-bar:focus-within {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-tint);
}

.request-bar .method-select {
  width: 116px;
  border: none;
  background: var(--bg-panel);
}
.request-bar .method-select :deep(.cs-trigger) {
  height: 100%;
  border: none;
  background: transparent;
  box-shadow: none;
  border-radius: 0;
}

.req-bar-divider {
  width: 1px;
  flex-shrink: 0;
  background: var(--border);
}

/* Tooltip 触发包裹层需允许收缩，避免挤压路径输入 */
.request-bar :deep(.tt-trigger) {
  min-width: 0;
}

/* 基础 URL 标签：紧凑无背景，仅 图标 + 环境名/域名，点击打开环境管理 */
.env-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  max-width: 140px;
  height: 100%;
  padding: 0 4px 0 10px;
  border: none;
  background: transparent;
  border-radius: 0;
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 600;
  line-height: 1;
  color: var(--text-2);
  cursor: pointer;
  transition:
    background var(--dur) var(--ease),
    color var(--dur) var(--ease);
}
.env-badge:hover {
  background: var(--bg-hover);
  color: var(--text-1);
}
.env-badge:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

/* 环境 base_url 变量已解析：主题色文字 */
.env-badge.env {
  color: var(--accent);
}
.env-badge.env:hover {
  color: var(--accent);
}

/* 变量未定义（将按字面量发送）：警告色文字 */
.env-badge.warn {
  color: var(--warning);
}
.env-badge.warn:hover {
  color: var(--warning);
}

/* 会话级 Base URL（未使用环境变量）：中性文字 */
.env-badge.session {
  color: var(--text-2);
}

/* ---- 模块绑定选择器（多模块环境下显示） ---- */
.mod-select {
  flex-shrink: 0;
}
.mod-select :deep(.cs-trigger) {
  height: 30px;
  border-color: var(--border);
  background: var(--bg-panel);
  font-size: 11.5px;
  color: var(--text-2);
  border-radius: 6px;
  padding: 0 8px;
}
.mod-select :deep(.cs-trigger:hover:not(:disabled)) {
  border-color: var(--border-strong);
  background: var(--bg-hover);
}
:global(.mod-pop .cs-pop) {
  min-width: 150px;
}
.env-badge.session:hover {
  color: var(--text-1);
}

.env-badge-icon {
  flex-shrink: 0;
  opacity: 0.8;
}

.env-badge-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.env-badge-chevron {
  flex-shrink: 0;
  opacity: 0.55;
  transition: opacity var(--dur) var(--ease);
}
.env-badge:hover .env-badge-chevron {
  opacity: 1;
}

.request-bar .url-input-wrap {
  position: relative;
  display: flex;
  align-items: center;
  flex: 1;
  min-width: 0;
  height: 100%;
}

.request-bar .url-input {
  flex: 1;
  min-width: 0;
  height: 100%;
  border: none;
  background: transparent;
  box-shadow: none;
  border-radius: 0;
  padding: 0 62px 0 10px;
  font-family: var(--font-mono);
}

/* 地址栏快捷按钮：悬浮输入框时浮现（Tooltip 触发器 span 承载绝对定位） */
.url-qbtn {
  position: absolute;
  top: 50%;
  right: 6px;
  transform: translateY(-50%);
  width: 26px;
  height: 26px;
  border-radius: 6px;
  color: var(--text-2);
  cursor: pointer;
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.url-qbtn-copy {
  right: 32px;
}
.url-qbtn:hover {
  background: var(--bg-hover);
  color: var(--text-1);
}
.url-qbtn-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  border: none;
  background: none;
  color: inherit;
  cursor: pointer;
  border-radius: 6px;
  padding: 0;
}

/* 请求栏右侧「发送」按钮：与输入组无缝贴合 */
.bar-send {
  height: 100%;
  flex-shrink: 0;
  border-radius: 0;
  padding: 0 16px;
  font-weight: 600;
}

/* ---- 面包屑行（接口名称移至此处） ---- */
.breadcrumb-row {
  gap: 8px;
}

.crumb {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  flex: 1;
  font-size: var(--fs-sm);
}

.crumb-part {
  color: var(--text-2);
  white-space: nowrap;
}

.crumb-sep {
  color: var(--text-3);
}

/* 接口标题：内联编辑样式——常态为面包屑文本，hover 显示虚线提示可编辑，聚焦高亮 */
.crumb-name {
  min-width: 60px;
  max-width: 280px;
  font-size: var(--fs-sm);
  font-weight: 600;
  color: var(--text-1);
  background: transparent;
  border: none;
  border-bottom: 1px dashed transparent;
  border-radius: 0;
  padding: 1px 2px;
  cursor: text;
  transition: border-bottom-color var(--dur) var(--ease);
}

.crumb-name:hover {
  border-bottom-color: var(--text-3);
  background: transparent;
}

.crumb-name:focus {
  outline: none;
  border-bottom-color: var(--accent);
  background: transparent;
}

.breadcrumb-spacer {
  flex: 1;
}

.editor-actions {
  display: flex;
  gap: 8px;
  flex-shrink: 0;
}

.config-box {
  display: flex;
  flex-direction: column;
  gap: 8px;
  flex-shrink: 0;
  min-height: 0;
  overflow-y: auto;
}

/* 无响应阶段：请求区占满剩余高度（body 大内容少滚动），响应仅留一行提示 */
.config-box.grow {
  flex: 1 1 auto;
}

.response-hint {
  margin: 0;
  padding: 10px 4px;
  border-top: 1px dashed var(--border);
  text-align: center;
  font-size: 12px;
  color: var(--text-3);
  user-select: none;
}

/* ---- 请求区 / 响应区分割条（Single Border Architecture：唯一分隔线）----
 * 请求编辑器底部、响应面板顶部均无边框；仅分割条提供 1px 视觉分隔。
 * 负 margin 抵消 .editor 的 gap，让请求编辑器底边紧贴分割条（2px），
 * 响应面板与分割条之间保留 6px 呼吸空间。
 */
.rp-splitter {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  height: 6px;
  flex-shrink: 0;
  margin: -10px 0 -6px;
  cursor: row-resize;
  user-select: none;
  touch-action: none;
}
.rp-splitter::before {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  top: 50%;
  transform: translateY(-50%);
  height: 1px;
  background: var(--border);
  transition:
    background var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}
.rp-splitter:hover::before,
.rp-splitter.dragging::before {
  background: var(--accent);
  box-shadow: 0 0 6px var(--accent);
}

/* 居中拖拽指示胶囊：默认隐约（opacity .4），Hover/拖拽时主题色高亮 */
.rp-splitter-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 12px;
  padding: 0;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: var(--bg-card);
  color: var(--text-3);
  opacity: 0.4;
  cursor: pointer;
  transition:
    opacity var(--dur) var(--ease),
    border-color var(--dur) var(--ease),
    color var(--dur) var(--ease);
}
.rp-splitter:hover .rp-splitter-btn,
.rp-splitter.dragging .rp-splitter-btn {
  opacity: 1;
  border-color: var(--accent);
  color: var(--accent);
}

.kv-remove {
  color: var(--rf-text-muted);
}

.body-mode-select {
  width: 200px;
}

.mp-type {
  width: 110px;
  flex-shrink: 0;
}

.oauth-form {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.oauth-hint {
  margin: 0;
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  color: var(--rf-text-muted);
}

.oauth-status.ok {
  color: var(--rf-success);
}

.response-save {
  margin-left: auto;
}

.examples {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.example-row {
  display: flex;
  align-items: center;
  gap: 6px;
}

.example-main {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 8px;
  border: 1px solid var(--rf-border);
  background: var(--rf-input-bg);
  border-radius: 6px;
  padding: 5px 10px;
  cursor: pointer;
  color: var(--rf-text, #f9fafb);
  font-size: 12.5px;
  text-align: left;
}

.example-main.open {
  border-color: var(--rf-info);
}

.example-status {
  font-weight: 700;
  font-size: 11px;
  color: var(--rf-success);
}

.example-status.err {
  color: var(--rf-danger);
}

.example-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.example-meta {
  font-size: 11px;
  color: var(--rf-text-muted);
}

.example-body {
  margin: 0;
  padding: 10px 12px;
  background: var(--rf-input-bg);
  border: 1px solid var(--rf-border);
  border-radius: 6px;
  font-family: var(--font-mono);
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 240px;
  overflow-y: auto;
}

.send-error {
  padding: 10px 12px;
  border-radius: var(--radius);
  background: var(--danger-tint);
  border: 1px solid var(--danger-border);
  color: var(--danger);
  font-size: 12.5px;
}

/* ---- 响应容器：flex:1 填满分割条以下所有空间 ---- */
.response-zone {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.response-empty {
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius);
  background: var(--bg-card);
}

.editor-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: var(--text-3);
}

/* ---- 名称输入对话框 ---- */

.name-hint {
  margin: 0 0 8px;
  font-size: 12.5px;
  color: var(--text-2);
}

.name-dialog-input {
  width: 100%;
  height: var(--h-md);
}

.folder-hint {
  margin-top: 12px;
}

.save-folder-select {
  width: 100%;
}

.save-folder-display {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>