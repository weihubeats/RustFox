<script setup lang="ts">
/**
 * EndpointEditor：接口编辑器（草稿模式）。
 *
 * - 直接编辑 store 草稿对象（Map 值经 Vue 集合响应式代理，嵌套修改即跟踪）；
 * - Base URL 为本地临时值（不落库），发送时与 path 拼接；
 * - 配置区为横向 Tab 系统：Params / Auth / Headers / Body / Examples / Code，
 *   各渲染独立面板组件（前置脚本 Scripts 与请求 Tab 的 Tests 已下线：
 *   断言迁至「工具」抽屉，代码生成即 Code 页签）；
 * - Ctrl+S 保存 / Ctrl+Enter 发送；响应区展示状态码、耗时与正文（JSON 自动美化）。
 */
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import { useWorkspaceStore } from '../stores/workspace'
import { useToast } from '../composables/useToast'
import { useFoxApi } from '../composables/useFoxApi'
import { useLocaleStore } from '../stores/locale'
import { isDefaultName } from '../stores/locale'
import { useShortcuts, shortcutDef } from '../composables/useShortcuts'
import {
  envBaseUrl,
  environmentVariableMap,
  resolveRequestUrl,
  resolveVariables,
  variableListToMap,
} from '../utils/environment'
import { isCurlCommand } from '../utils/url'
import { useVarCandidates } from '../composables/useVarCandidates'
import { useVarAutocomplete } from '../composables/useVarAutocomplete'
import { lazyComponent } from '../composables/lazyComponent'
import { deepClone } from '../utils/clone'
import {
  applyMethodDefaults,
  envBadgeLabel as envBadgeLabelOf,
  methodNeedsBody,
} from '../utils/requestBar'
import AuthPanel from './AuthPanel.vue'
import BodyPanel from './BodyPanel.vue'
import CodeExportMenu from './CodeExportMenu.vue'
import CodePanel from './CodePanel.vue'
import HeadersPanel from './HeadersPanel.vue'
import CustomNumberInput from './ui/CustomNumberInput.vue'
import CustomSelect from './ui/CustomSelect.vue'
import EmptyState from './ui/EmptyState.vue'
import Skeleton from './ui/Skeleton.vue'
import Icon from './ui/Icon.vue'
import IconButton from './ui/IconButton.vue'
import Menu, { type MenuItem } from './ui/Menu.vue'
import Modal from './ui/Modal.vue'
import ParamsPanel from './ParamsPanel.vue'
import PathVariablesPanel from './PathVariablesPanel.vue'
import Popconfirm from './ui/Popconfirm.vue'
import ResponsePanel from './ResponsePanel.vue'
import RequestExamplesPanel from './RequestExamplesPanel.vue'
import Tabs from './ui/Tabs.vue'
import TestCaseModal from './TestCaseModal.vue'
import Tooltip from './ui/Tooltip.vue'
import ToolsDrawer from './ToolsDrawer.vue'
import VarSuggest from './ui/VarSuggest.vue'
import type { TabItem } from './ui/Tabs.vue'
import type {
  ExecuteResponse,
  Environment,
  HttpMethod,
  RequestSpec,
  ResponseExample,
  TestCaseCategory,
} from '../types/foxApi'

const store = useWorkspaceStore()
const toast = useToast()
const api = useFoxApi()
const locale = useLocaleStore()
const t = locale.t

// 低频 / 重型面板按需加载（composables/lazyComponent.ts：骨架 + 可翻译的错误重试层）。
// 切视图 / 开弹窗才出现的：Design、Mock 视图与 Mock 规则弹窗；Docs / 用例视图内部链路
// 引入 CodeMirror 全家桶（约 300KB），首次切到才拉。EnvironmentManager 常驻但不在首屏
// 关键路径——异步化把它挪出主 chunk（挂载后后台取），不加 v-if 以免丢掉弹窗进出场动画。
const DesignPanel = lazyComponent(() => import('./DesignPanel.vue'))
const MockPanel = lazyComponent(() => import('./MockPanel.vue'))
const MockRuleDialog = lazyComponent(() => import('./MockRuleDialog.vue'))
const EnvironmentManager = lazyComponent(() => import('./EnvironmentManager.vue'))
const DocsPanel = lazyComponent(() => import('./DocsPanel.vue'))
const TestCasesPanel = lazyComponent(() => import('./TestCasesPanel.vue'))

const sendingMap = ref<Map<string, { requestId: string; startedAt: number }>>(new Map())

/** 当前接口是否在途（逐接口隔离：A 发送中切到 B，B 仍可独立发送）。 */
const sending = computed(() => (draft.value ? sendingMap.value.has(draft.value.id) : false))
/** 在途请求的取消标识（非空表示有请求可取消）。 */
const activeRequestId = computed(() =>
  draft.value ? (sendingMap.value.get(draft.value.id)?.requestId ?? null) : null,
)
/** 驱动「发送中计时」重绘的心跳（Apifox 式实时计时，仅在途时推进）。 */
const elapsedTick = ref(0)
let elapsedTimer: ReturnType<typeof setInterval> | undefined

function ensureElapsedTimer(): void {
  if (elapsedTimer) return
  elapsedTimer = setInterval(() => {
    if (sendingMap.value.size === 0) {
      clearInterval(elapsedTimer)
      elapsedTimer = undefined
      return
    }
    elapsedTick.value += 1
  }, 100)
}

/** 当前在途耗时（ms），靠 elapsedTick 心跳刷新。 */
const elapsedMs = computed(() => {
  // eslint-disable-next-line @typescript-eslint/no-unused-expressions
  elapsedTick.value
  if (!draft.value) return 0
  const f = sendingMap.value.get(draft.value.id)
  return f ? Math.max(0, Date.now() - f.startedAt) : 0
})
/** 发送中实时计时文案（Apifox 式：0.0s 起跳，持续跳动）。 */
const elapsedText = computed(() => `${(elapsedMs.value / 1000).toFixed(1)}s`)
/** 响应体动画锚点：新响应到达时重启动画，旧响应在途时降 dim——仅视觉反馈，不新增展示块。 */
const flashEl = ref<HTMLElement | null>(null)

function triggerFlash(): void {
  const el = flashEl.value
  if (!el) return
  el.classList.remove('flash')
  void el.offsetWidth
  el.classList.add('flash')
}
/**
 * 各接口的请求结果按 id 分桶：EndpointEditor 是单实例常驻，若用单个 ref，
 * 切到另一个接口会看到上一个接口的响应；且请求在途时切换接口，旧接口的
 * 返回也会落到当前接口上（偶发「A 请求却显示 B 结果」）。按 id 存取后，
 * 每个接口只显示自己的最后一次结果，天然隔离上述两处错位。
 */
const responses = ref<Map<string, ExecuteResponse | null>>(new Map())
const sendErrors = ref<Map<string, string | null>>(new Map())
const response = computed<ExecuteResponse | null>(() =>
  draft.value ? (responses.value.get(draft.value.id) ?? null) : null,
)
const sendError = computed<string | null>(() =>
  draft.value ? (sendErrors.value.get(draft.value.id) ?? null) : null,
)

const draft = computed(() => store.activeEndpoint)

const METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS']
const METHOD_OPTIONS = METHODS.map((m) => ({ value: m, label: m }))

// ---------- 配置 Tab 系统 ----------
type ConfigTabKey =
  | 'params'
  | 'auth'
  | 'headers'
  | 'body'
  | 'path'
  | 'examples'
  | 'code'

/** 合法 Tab 集合（历史数据可能持久化过已下线的前置脚本 Scripts / Tests 页签）。 */
const VALID_TABS: readonly ConfigTabKey[] = ['params', 'auth', 'headers', 'body', 'path', 'examples', 'code']

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
    { key: 'params', label: t('editor.tabParams'), count: d.request.params.length },
    { key: 'auth', label: t('editor.tabAuth') },
    { key: 'headers', label: t('editor.tabHeaders'), count: d.request.headers.length },
    {
      key: 'body',
      label:
        bodyMode !== 'none'
          ? `${t('editor.tabBody')} (${BODY_TAB_LABELS[bodyMode] ?? bodyMode})`
          : t('editor.tabBody'),
      count: bodyMode !== 'none' ? 1 : undefined,
    },
    { key: 'path', label: t('editor.tabPath'), count: d.request.path_variables?.length ?? 0 },
    { key: 'examples', label: t('editor.tabExamples') },
    { key: 'code', label: t('editor.tabCode') },
  ]
})

/** 接口切换：无保存的 active_tab 时按 Method 设定智能默认（不落库）。
 * 请求结果按 id 分桶（responses/sendErrors Map），切换接口天然隔离，无需清空。 */
watch(
  () => draft.value?.id,
  () => {
    smartTab.value = draft.value && methodNeedsBody(draft.value.method) ? 'body' : 'params'
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
    methodRevert ??= { from: prev?.[1] ?? m, snapshot: deepClone(d.request) }
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

/** 激活环境（chip 色点）。 */
const activeEnv = computed(
  () => store.environments.find((e) => e.id === store.activeEnvId) ?? null,
)
const activeEnvName = computed(() => activeEnv.value?.name ?? '')

/** 环境变量 + 项目变量 + 全局变量合并表（chips / 预览解析用；优先级 环境 > 项目 > 全局）。 */
const envVars = computed(() => ({
  ...variableListToMap(store.globalVariables),
  ...(store.project?.variables ?? {}),
  ...environmentVariableMap(activeEnv.value, store.project?.id),
}))

/** 地址栏前缀 chip 文案：环境 base_url 变量的「解析后」实际值或会话 Base URL。 */
const resolvedDomain = computed(() => {
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

/** Base URL 下拉「管理环境…」→ 打开环境管理。 */
const showEnvManager = ref(false)
const showMockRules = ref(false)

/** 地址栏 Base URL 下拉（Apifox 式：前置 URL 选择器）。 */
const BASE_URL_MANAGE = '__manage__'
const baseUrlMenuOpen = ref(false)

const sessionBaseVars = computed(() => ({
  ...variableListToMap(store.globalVariables),
  ...(store.project?.variables ?? {}),
}))

function resolveEnvBaseUrl(env: Environment): string {
  const vars = {
    ...sessionBaseVars.value,
    ...environmentVariableMap(env, store.project?.id),
  }
  const raw = envBaseUrl(env, store.project?.id)
  return raw ? resolveVariables(raw, vars) : ''
}

/** 下拉选项：`''`=无环境（会话 Base URL），各环境=解析后前置 URL，末项=管理环境。 */
const baseUrlOptions = computed(() => {
  const opts: { value: string; label: string }[] = []
  const session = store.sessionBaseUrl
    ? resolveVariables(store.sessionBaseUrl, sessionBaseVars.value)
    : ''
  opts.push({ value: '', label: session || t('envbar.noEnv') })
  for (const env of store.environments) {
    opts.push({ value: env.id, label: resolveEnvBaseUrl(env) || t('envbar.noEnv') })
  }
  opts.push({ value: BASE_URL_MANAGE, label: t('envbar.manage') })
  return opts
})

/** 下拉行右侧标签：环境名 / 无环境。 */
function baseUrlSideLabel(value: string): string {
  if (value === BASE_URL_MANAGE) return ''
  if (!value) return t('envbar.noEnv')
  return store.environments.find((e) => e.id === value)?.name ?? ''
}

function onBaseUrlChange(value: string | number): void {
  const v = String(value)
  if (v === BASE_URL_MANAGE) {
    showEnvManager.value = true
    return
  }
  void store.setEnvironment(v === '' ? null : v)
}

/** Base URL 紧凑标签：直接展示解析后的裸域名（无域名时退回环境名），一眼可见实际发送目标。 */
const envBadgeLabel = computed(() =>
  envBadgeLabelOf({
    urlDomain: urlDomain.value,
    resolvedDomain: resolvedDomain.value,
    envName: activeEnvName.value,
  }),
)



/** 路径输入框元素引用（快捷按钮聚焦回跳）。 */
const urlInputEl = ref<HTMLInputElement | null>(null)

/** 地址栏 {{变量}} 候选：内置变量 + 环境/项目/全局变量（随 store 变化刷新）。 */
const varCandidates = useVarCandidates()

/** 地址栏 {{ 自动补全（↑↓ 选择、Enter/Tab 插入、Esc 关闭）。 */
const urlAc = useVarAutocomplete(varCandidates)
const { open: urlAcOpen, items: urlAcItems, activeIndex: urlAcIndex, anchor: urlAcAnchor } = urlAc

/** 输入 / 点击（重设光标）后刷新补全状态。 */
function onUrlAcSync(event: Event): void {
  urlAc.onInput(event.target as HTMLInputElement)
}

/** 弹层候选项 → 插入 `{{name}}`（composable 内 dispatch input 同步 v-model）。 */
function onUrlAcPick(index: number): void {
  const el = urlInputEl.value
  if (el) urlAc.pick(el, index)
}

/** 地址栏 cURL 粘贴解析中（防重复触发）。 */
const curlPasting = ref(false)

/** 归一化 shell 续行 + 去终端提示符，与 CurlImportDialog 保持一致。 */
function normalizeCurlCommand(raw: string): string {
  return raw
    .replace(/^\s*[$#>]+[ \t]+/, '')
    .replace(/\\[ \t]*\r?\n/g, ' ')
    .trim()
}

/** 地址栏粘贴 cURL 命令 → 解析后回填当前草稿（method / URL / headers / body / auth）。 */
async function importCurlText(raw: string): Promise<void> {
  if (!draft.value || curlPasting.value) return
  const targetId = draft.value.id
  curlPasting.value = true
  try {
    const parsed = await api.parseCurlCommand(normalizeCurlCommand(raw))
    if (draft.value?.id !== targetId) return
    store.applyCurlToDraft(targetId, parsed)
    if (parsed.ignored?.length) {
      toast.success(t('editor.curlAutoImported'), {
        message: t('curldlg.ignoredHint', { v: parsed.ignored.join(' ') }),
      })
    } else {
      toast.success(t('editor.curlAutoImported'))
    }
  } catch (err) {
    toast.error(t('editor.curlParseFail', { v: err instanceof Error ? err.message : String(err) }))
  } finally {
    curlPasting.value = false
  }
}

/** 地址栏粘贴：cURL 命令自动识别并解析，其余走常规 URL 拆分（urlPath setter）。 */
function onUrlPaste(event: ClipboardEvent): void {
  const text = event.clipboardData?.getData('text') ?? ''
  if (!text || !draft.value || curlPasting.value) return
  if (!isCurlCommand(text)) return
  event.preventDefault()
  void importCurlText(text)
}

/** 路径输入框 placeholder：有基础 URL 时提示自动拼接，无则提示粘贴完整 URL。 */
const urlPlaceholder = computed(() => {
  if (!urlDomain.value) return t('editor.urlPhBare')
  return t('editor.urlPhJoin', { v: resolvedDomain.value || urlDomain.value })
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

    // 1) 粘贴/改写完整 URL：query 并入参数；展示前缀为环境变量时 origin+path
    //    整条存入 path（发送走 isAbsolutePath 分支，不覆写共享环境），否则 origin 写会话 Base URL。
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
      const envPrefixed = store.urlDomain.startsWith('{{')
      store.sessionBaseUrl = abs[0]
      const rel = rest.startsWith('/') ? rest : `/${rest}`
      d.path = envPrefixed ? `${abs[0]}${rel}` : rel
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

/**
 * 路径变量代入（镜像 fox-core util::replace_path_variables：`{key}` 与
 * `{{key}}` 两种写法，长 key 先替换避免前缀冲突）。
 *
 * 后端 execute_request 直接使用前端拼好的完整 URL，不会消费
 * request.path_variables（该字段目前仅在 OpenAPI 导出与文档预览中读取），
 * 故代入放在前端 buildUrl：仅替换「已启用且取值非空」的行，
 * 未配置时路径原样发送（历史行为不变）。
 */
function applyPathVariables(path: string, pathVars: RequestSpec['path_variables']): string {
  const rows = (pathVars ?? [])
    .filter((r) => r.enabled !== false && r.key && r.value)
    .sort((a, b) => b.key.length - a.key.length)
  let out = path
  for (const row of rows) {
    out = out.split(`{{${row.key}}}`).join(row.value).split(`{${row.key}}`).join(row.value)
  }
  return out
}

/** 请求地址（与 send / 代码生成 / 压测共用）；有环境时按默认模块（优先当前项目绑定）拼接，变量由 resolveRequestUrl 解析。 */
function buildUrl(): string {
  const d = draft.value
  if (!d) return ''
  const path = applyPathVariables(d.path, d.request.path_variables)
  if (isAbsolutePath(path)) return path
  if (activeEnv.value && envBaseUrl(activeEnv.value)) {
    return resolveRequestUrl(activeEnv.value, null, path, envVars.value, d.project_id).url
  }
  const rel = path.startsWith('/') ? path : `/${path}`
  return `${store.urlDomain}${rel}`
}

/** 单请求超时输入：空串 = null（跟随全局超时），非法输入不写回（避免打字中途被清空）。 */
function onTimeoutInput(value: string | number): void {
  const request = draft.value?.request
  if (!request) return
  if (value === '' || value === null || value === undefined) {
    request.timeout_ms = null
    return
  }
  const n = typeof value === 'number' ? value : Number(value)
  if (Number.isFinite(n) && n > 0) request.timeout_ms = Math.round(n)
}

async function send(): Promise<void> {
  if (!draft.value) return
  const targetId = draft.value.id
  if (sendingMap.value.has(targetId)) {
    toast.info(t('editor.sendingHint'))
    return
  }
  const snapshot = draft.value
  sendErrors.value.set(targetId, null)
  const url = buildUrl()
  const rid = crypto.randomUUID()
  sendingMap.value.set(targetId, { requestId: rid, startedAt: Date.now() })
  ensureElapsedTimer()
  try {
    const resp = await store.send(snapshot, url, rid)
    // 结果按发起请求时的接口 id 落桶：请求在途时切走再返回，不会错位。
    responses.value.set(targetId, resp)
    sendErrors.value.set(targetId, null)
    triggerFlash()
    // 历史已迁至侧栏「请求历史」页签，发送后由 store 统一刷新。
    void store.loadHistories()
  } catch (err) {
    const e = err as Error & { code?: string }
    if (e?.code === 'CANCELLED') {
      // 用户主动取消：不视为错误，保留上一次结果。
      toast.info(t('editor.cancelled'))
      sendErrors.value.set(targetId, null)
    } else {
      sendErrors.value.set(targetId, err instanceof Error ? err.message : String(err))
      responses.value.set(targetId, null)
      triggerFlash()
    }
  } finally {
    sendingMap.value.delete(targetId)
    // 等响应分支挂载后再重启动画：完成瞬间仍是请求中占位，flashEl 为空。
    await nextTick()
    triggerFlash()
  }
}

/** 取消在途请求（后端中止连接，命令随即以 CANCELLED 返回）。 */
function cancelSend(): void {
  const rid = activeRequestId.value
  if (!rid) return
  void api.cancelRequest(rid)
  toast.info(t('editor.cancelling'))
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
  if (isDefaultName('endpoint', name)) {
    pendingName.value = ''
    pendingFolderId.value = draft.value.folder_id ?? ''
    showNameDialog.value = true
    return
  }
  const ok = await store.saveActiveDraft()
  if (ok) {
    methodRevert = null
  }
}

async function confirmName(): Promise<void> {
  if (!draft.value) return
  const name = pendingName.value.trim()
  if (!name) {
    toast.warning(t('editor.nameRequired'))
    return
  }
  draft.value.name = name
  draft.value.folder_id = pendingFolderId.value || null
  showNameDialog.value = false
  const ok = await store.saveActiveDraft()
  if (ok) {
    methodRevert = null
  }
}

// ---------- 二级导航 + 保存为用例 ----------
const SUB_NAV = computed<{ key: 'debug' | 'design' | 'docs' | 'cases' | 'mock'; label: string }[]>(() => [
  { key: 'debug', label: t('editor.navDebug') },
  { key: 'design', label: t('editor.navDesign') },
  { key: 'docs', label: t('editor.navDocs') },
  { key: 'cases', label: t('editor.navCases') },
  { key: 'mock', label: 'Mock' },
])

/** 切换接口时回到「调试」页。 */
watch(
  () => draft.value?.id,
  () => store.setActiveView('debug'),
  { immediate: true },
)

const saveMenuEl = ref<InstanceType<typeof Menu> | null>(null)

function openSaveMenu(event: MouseEvent): void {
  saveMenuEl.value?.openAt(event.currentTarget as HTMLElement, [
    { key: 'save-case', label: t('editor.saveAsCase'), icon: 'list' },
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
  pendingCaseName.value = !isDefaultName('endpoint', d.name) ? `${d.method} ${d.path}` : ''
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

/** 正在发送或已有结果时展示响应区（发送中显示请求中占位，见模板）。 */
const hasResponse = computed(() => !!response.value || !!sendError.value || sending.value)

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

/** 键盘调高（分割条可 Tab 聚焦）：↑ ↓ 每次 10px，Shift 40px，PgUp/PgDn 更大步长，Home/End 到上下限。 */
function onSplitterKeydown(event: KeyboardEvent): void {
  const max = requestMaxHeight()
  const step = event.shiftKey ? 40 : 10
  let next = requestBodyHeight.value
  if (event.key === 'ArrowUp') next -= step
  else if (event.key === 'ArrowDown') next += step
  else if (event.key === 'PageUp') next -= step * 4
  else if (event.key === 'PageDown') next += step * 4
  else if (event.key === 'Home') next = REQUEST_MIN
  else if (event.key === 'End') next = max
  else return
  event.preventDefault()
  requestBodyHeight.value = Math.min(Math.max(next, REQUEST_MIN), max)
}

// ---------- 响应示例 ----------
const viewingExample = ref<ResponseExample | null>(null)
const activeExamples = computed(() => store.examples.get(draft.value?.id ?? '') ?? [])

const showExampleDialog = ref(false)
const exampleName = ref('')

function saveExample(): void {
  if (!draft.value || !response.value) return
  exampleName.value = `${draft.value.method} ${new Date().toLocaleTimeString(locale.resolved === 'zh' ? 'zh-CN' : 'en-US')}`
  showExampleDialog.value = true
}

async function confirmSaveExample(): Promise<void> {
  if (!draft.value || !response.value) return
  const name = exampleName.value.trim()
  if (!name) {
    toast.warning(t('editor.exampleNameRequired'))
    return
  }
  try {
    await store.saveAsExample(draft.value.id, name, response.value)
    showExampleDialog.value = false
  } catch (err) {
    toast.error(t('editor.exampleSaveFail'), { message: err instanceof Error ? err.message : String(err) })
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
    toast.error(t('editor.exampleDeleteFail'), { message: err instanceof Error ? err.message : String(err) })
  }
}

// ---------- 工具抽屉（生成代码 / 测试 / 压测） ----------
const showTools = ref(false)

const requestUrl = computed(() => (draft.value ? buildUrl() : ''))

/** 路径输入框 Enter → 发送；Esc → 清空路径（setter 忽略空串，直接写草稿）。 */
function onUrlKeydown(event: KeyboardEvent): void {
  const el = event.target as HTMLInputElement
  // 补全弹层打开时接管 ↑↓/Enter/Tab/Esc，避免 Enter 直接触发发送
  if (urlAc.handleKeydown(event, el)) return
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
  toast.info(t('common.copied'))
}

/** 快捷按钮：复制当前 Base URL（解析后优先，未解析退回字面量）。 */
async function copyBaseUrl(): Promise<void> {
  const v = resolvedDomain.value || urlDomain.value
  if (!v) return
  await navigator.clipboard.writeText(v)
  toast.info(t('editor.baseCopied'))
}

/**
 * 全局快捷键（集中注册表，见 useShortcuts；帮助面板自动收录）。
 * 默认键位来自 SHORTCUT_DEFAULTS，用户可在设置 → 快捷键中自定义。
 * inInput: true 保持原行为——原来是裸 window 监听，输入框内同样生效。
 */
useShortcuts([
  shortcutDef('editor.save', () => save()),
  shortcutDef('editor.send', () => void send()),
  shortcutDef('editor.new-request-t', () => store.openNewEndpoint(null)),
  shortcutDef('editor.new-request-n', () => store.openNewEndpoint(null)),
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
  if (elapsedTimer) {
    clearInterval(elapsedTimer)
    elapsedTimer = undefined
  }
})
</script>

<template>
  <div ref="editorEl" v-if="draft" class="editor">
    <div class="editor-row breadcrumb-row">
      <span class="crumb">
        <span class="crumb-part">{{ store.project?.name ?? t('default.projectName') }}</span>
        <template v-if="folderName">
          <span class="crumb-sep">/</span>
          <span class="crumb-part">{{ folderName }}</span>
        </template>
        <span class="crumb-sep">/</span>
        <input
          v-model="draft.name"
          class="crumb-name"
          :placeholder="t('editor.endpointNamePh')"
          spellcheck="false"
          :title="t('editor.endpointNameHint')"
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
      <div class="request-bar" :class="{ 'is-sending': sending }">
        <div v-if="sending" class="req-progress" aria-hidden="true"></div>
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
        <span v-if="!isAbsolutePath(draft.path)" class="req-bar-divider"></span>
        <div v-if="!isAbsolutePath(draft.path)" class="base-url-wrap">
          <Tooltip
            :content="t('editor.baseUrlHint')"
            placement="bottom"
            :disabled="baseUrlMenuOpen"
            class="base-url-tip"
          >
            <CustomSelect
              class="base-url-select"
              :class="chipClass"
              pop-class="base-url-pop"
              :pop-min-width="360"
              :model-value="store.activeEnvId ?? ''"
              :options="baseUrlOptions"
              :placeholder="t('envbar.noEnv')"
              @change="onBaseUrlChange"
              @open="baseUrlMenuOpen = true"
              @close="baseUrlMenuOpen = false"
            >
              <template #display>
                <Icon name="globe" :size="13" class="env-badge-icon" />
                <span class="env-badge-text">{{ envBadgeLabel }}</span>
              </template>
              <template #option="{ option }">
                <span v-if="option.value === BASE_URL_MANAGE" class="base-url-manage">
                  {{ option.label }}
                </span>
                <template v-else>
                  <span class="base-url-opt-url">{{ option.label }}</span>
                  <span class="base-url-opt-name">{{ baseUrlSideLabel(String(option.value)) }}</span>
                </template>
              </template>
            </CustomSelect>
          </Tooltip>
          <Tooltip
            v-if="resolvedDomain || urlDomain"
            :content="t('editor.copyBaseUrl')"
            placement="bottom"
            :disabled="baseUrlMenuOpen"
            class="base-url-copy"
          >
            <button type="button" class="base-url-copy-btn" @click.stop="copyBaseUrl">
              <Icon name="copy" :size="12" />
            </button>
          </Tooltip>
        </div>
        <div class="url-input-wrap">
          <input
            ref="urlInputEl"
            v-model="urlPath"
            class="url-input"
            spellcheck="false"
            :placeholder="urlPlaceholder"
            @keydown="onUrlKeydown"
            @paste="onUrlPaste"
            @input="onUrlAcSync"
            @click="onUrlAcSync"
            @blur="urlAc.close"
          />
          <VarSuggest
            v-if="urlAcOpen"
            :anchor="urlAcAnchor"
            :items="urlAcItems"
            :active-index="urlAcIndex"
            @pick="onUrlAcPick"
          />
          <template v-if="urlPath">
            <Tooltip :content="t('editor.copyUrl')" placement="top" class="url-qbtn url-qbtn-copy">
              <button type="button" class="url-qbtn-btn" @click="copyRequestUrl">
                <Icon name="copy" :size="13" />
              </button>
            </Tooltip>
            <Tooltip :content="t('editor.clearPath')" placement="top" class="url-qbtn">
              <button type="button" class="url-qbtn-btn" @click="clearPath">
                <Icon name="x" :size="13" />
              </button>
            </Tooltip>
          </template>
        </div>
        <button v-if="!sending" class="rf-btn rf-btn-send bar-send" type="button" @click="send">
          <Icon name="send" :size="14" />
          {{ t('editor.send') }}
        </button>
        <button
          v-else
          class="rf-btn rf-btn-danger bar-send is-sending"
          type="button"
          :title="t('editor.sendingCancel')"
          @click="cancelSend"
        >
          <span class="btn-spinner" aria-hidden="true"></span>
          <span>{{ t('editor.sending') }} <span class="bar-send-elapsed">{{ elapsedText }}</span></span>
          <Icon name="stop" :size="13" />
        </button>
      </div>
      <div class="editor-actions">
        <button class="rf-btn rf-btn-sm" type="button" :title="t('editor.toolsHint')" @click="showTools = true">
          <Icon name="gauge" :size="13" /> {{ t('editor.tools') }}
        </button>
        <CodeExportMenu :draft="draft" :url="requestUrl" />
        <div class="save-group">
          <button class="rf-btn save-main" type="button" @click="save">
            <Icon name="save" :size="14" /> {{ t('editor.saveHint') }}
          </button>
          <button
            class="rf-btn save-arrow"
            type="button"
            :title="t('editor.saveMore')"
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
      <PathVariablesPanel v-else-if="activeTab === 'path'" :draft="draft" />
      <RequestExamplesPanel v-else-if="activeTab === 'examples'" :draft="draft" />
      <CodePanel v-else :draft="draft" :url="requestUrl" />

      <!-- 配置区底部：单请求超时 / 跟随重定向（绑定 draft.request，走既有脏检查与保存链路） -->
      <div class="req-settings">
        <span class="rs-label">{{ t('editor.reqSettings') }}</span>
        <label class="rs-field" :title="t('editor.timeoutPh')">
          <span class="rs-text">{{ t('editor.timeoutLabel') }}</span>
          <CustomNumberInput
            class="rs-timeout"
            size="sm"
            :model-value="draft.request.timeout_ms ?? ''"
            :placeholder="t('editor.timeoutPh')"
            @update:model-value="onTimeoutInput"
          />
          <span class="rs-unit">{{ t('editor.msUnit') }}</span>
        </label>
        <label class="rs-check" :title="t('editor.followRedirects')">
          <input
            :checked="draft.request.follow_redirects"
            type="checkbox"
            @change="draft.request.follow_redirects = ($event.target as HTMLInputElement).checked"
          />
          {{ t('editor.followRedirects') }}
        </label>
      </div>
    </div>

    <template v-if="hasResponse">
      <div
        class="rp-splitter"
        :class="{ dragging: splitterDragging }"
        role="separator"
        tabindex="0"
        aria-orientation="horizontal"
        :aria-label="t('editor.splitterHint')"
        :aria-valuenow="Math.round(requestBodyHeight)"
        :aria-valuemin="REQUEST_MIN"
        :aria-valuemax="Math.round(requestMaxHeight())"
        :title="t('editor.splitterHint')"
        @mousedown="onSplitterDown"
        @dblclick="toggleRequestBody"
        @keydown="onSplitterKeydown"
      >
        <button
          class="rp-splitter-btn"
          type="button"
          :title="requestBodyCollapsed ? t('editor.expandRequest') : t('editor.collapseRequest')"
          @mousedown.stop
          @dblclick.stop
          @click="toggleRequestBody"
        >
          <Icon :name="requestBodyCollapsed ? 'chevron-up' : 'chevron-down'" :size="11" />
        </button>
      </div>

      <div class="response-zone">
        <!-- Apifox 式请求中占位：转圈 + 实时计时 + 骨架 shimmer，一眼可知「这次发出去了」。 -->
        <div v-if="sending && !response && !sendError" class="req-loading" role="status" aria-live="polite">
          <span class="req-loading-ring" aria-hidden="true"></span>
          <p class="req-loading-title">{{ t('editor.sendingTitle') }}</p>
          <p class="req-loading-sub">{{ draft.method }} {{ requestUrl }}</p>
          <div class="req-loading-skel">
            <Skeleton :lines="5" height="12px" />
          </div>
          <button class="rf-btn rf-btn-sm req-loading-cancel" type="button" @click="cancelSend">
            {{ t('editor.cancelRequest') }}
          </button>
        </div>
        <div v-else ref="flashEl" class="response-anim" :class="{ 'is-stale': sending }">
          <ResponsePanel v-if="response" :response="response" @save-example="saveExample" />
          <div v-else-if="sendError" class="send-error" role="alert">
            <span>{{ t('editor.sendFail', { v: sendError }) }}</span>
          </div>
          <EmptyState
            v-else
            class="response-empty"
            icon="send"
            :title="t('editor.notSent')"
            :description="t('editor.notSentHint')"
          />
        </div>
      </div>
    </template>
    <p v-else class="response-hint">{{ t('editor.responseHint') }}</p>
    <div v-if="activeExamples.length" class="examples">
      <h3 class="section-title">{{ t('editor.examples', { n: activeExamples.length }) }}</h3>
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
        <Popconfirm :title="t('editor.deleteExampleConfirm', { name: ex.name })" @confirm="removeExample(ex)">
            <IconButton name="trash" :size="13" tone="danger" :title="t('editor.deleteExample')" />
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
    <p>{{ t('editor.empty') }}</p>
  </div>

  <Modal v-model:open="showNameDialog" :title="t('editor.saveEndpoint')" width="420px">
    <p class="name-hint">{{ t('editor.nameHint') }}</p>
    <input
      v-model="pendingName"
      class="rf-input name-dialog-input"
      :placeholder="t('editor.namePh')"
      spellcheck="false"
      @keyup.enter="confirmName"
    />
    <p class="name-hint folder-hint">{{ t('editor.saveFolderHint') }}</p>
    <CustomSelect
      class="save-folder-select"
      :model-value="pendingFolderId"
      :options="folderOptions"
      :placeholder="t('editor.saveFolderPh')"
      @update:model-value="pendingFolderId = String($event)"
    >
      <template #display="{ label }">
        <span class="save-folder-display">{{ label || t('editor.saveFolderPh') }}</span>
      </template>
      <template #option="{ option }">
        <span :style="{ paddingLeft: `${(option as FolderOption).depth * 16 + 4}px` }">
          {{ option.label }}
        </span>
      </template>
    </CustomSelect>
    <template #footer>
      <button class="rf-btn" type="button" @click="showNameDialog = false">{{ t('common.cancel') }}</button>
      <button class="rf-btn rf-btn-primary" type="button" @click="confirmName">
        <Icon name="save" :size="14" /> {{ t('common.save') }}
      </button>
    </template>
  </Modal>

  <Modal v-model:open="showExampleDialog" :title="t('editor.saveExampleTitle')" width="360px">
    <p class="name-hint">{{ t('editor.exampleNameHint') }}</p>
    <input
      v-model="exampleName"
      class="rf-input name-dialog-input"
      :placeholder="t('editor.exampleNamePh')"
      spellcheck="false"
      @keyup.enter="confirmSaveExample"
    />
    <template #footer>
      <button class="rf-btn" type="button" @click="showExampleDialog = false">{{ t('common.cancel') }}</button>
      <button class="rf-btn rf-btn-primary" type="button" @click="confirmSaveExample">{{ t('common.save') }}</button>
    </template>
  </Modal>

  <ToolsDrawer :open="showTools" :draft="draft" :url="requestUrl" @close="showTools = false" />

  <EnvironmentManager v-model:open="showEnvManager" />

  <TestCaseModal
    :open="showTestCaseModal"
    :title="t('editor.saveAsCase')"
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
  border-radius: var(--radius-md);
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
  border-left: 1px solid var(--border-strong);
  padding: 0 7px;
}

.method-select {
  width: 108px;
  flex-shrink: 0;
  font-weight: 700;
}

/* m-select-* 方法文本色已上收全局（style.css），此处不再重复定义。 */

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

/* Base URL 前置选择器（Apifox 式）：占满请求栏高度，点击展开环境基址列表 */
.base-url-wrap {
  position: relative;
  display: inline-flex;
  align-self: stretch;
  min-width: 0;
  max-width: 300px;
}
.base-url-tip {
  display: inline-flex;
  flex: 1;
  min-width: 0;
}
.request-bar .base-url-select {
  flex: 1;
  min-width: 0;
  max-width: none;
}
.request-bar .base-url-select :deep(.cs-trigger) {
  height: 100%;
  gap: 6px;
  border: none;
  background: transparent;
  border-radius: 0;
  box-shadow: none;
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 600;
  color: var(--text-2);
  /* 右侧预留复制按钮 + caret 位，防域名文字钻入按钮下 */
  padding: 0 44px 0 10px;
}
.request-bar .base-url-select :deep(.cs-trigger:hover) {
  background: var(--bg-hover);
  color: var(--text-1);
}
.request-bar .base-url-select :deep(.cs.open .cs-trigger) {
  box-shadow: none;
}
.request-bar .base-url-select :deep(.cs-value) {
  color: inherit;
}
.request-bar .base-url-select.env :deep(.cs-value) {
  color: var(--accent);
}
.request-bar .base-url-select.warn :deep(.cs-value) {
  color: var(--warning);
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

/* Base URL 复制按钮：悬浮在触发器右侧（caret 左），不挤占域名展示宽度 */
.base-url-copy {
  position: absolute;
  top: 50%;
  right: 20px;
  transform: translateY(-50%);
  display: inline-flex;
  z-index: 1;
}
.base-url-copy-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: 5px;
  padding: 0;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  transition: background var(--dur) var(--ease), color var(--dur) var(--ease);
}
.base-url-copy-btn:hover {
  background: var(--bg-hover);
  color: var(--text-1);
}

/* 下拉选项：左前置 URL，右环境名（Apifox 样式） */
:global(.cs-pop.base-url-pop) {
  padding: 6px;
  border-radius: var(--radius-lg);
}
:global(.cs-pop.base-url-pop .cs-opt) {
  height: auto;
  min-height: 34px;
  padding: 6px 8px;
  gap: 12px;
  border-radius: var(--radius-sm);
  font-family: var(--font-ui);
  white-space: normal;
}
:global(.cs-pop.base-url-pop .cs-opt.hl) {
  background: var(--bg-hover);
}
/* 选中：accent 轻底 + 对勾高亮，文字保持正文色（默认 .sel 会整行染 accent） */
:global(.cs-pop.base-url-pop .cs-opt.sel) {
  color: var(--text-1);
  background: var(--accent-tint);
}
:global(.cs-pop.base-url-pop .cs-opt.sel .cs-opt-check) {
  color: var(--accent);
}
:global(.cs-pop.base-url-pop .cs-opt-check) {
  width: 16px;
}
:global(.cs-pop.base-url-pop .cs-opt-label) {
  display: flex;
  align-items: center;
  gap: 16px;
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
}
.base-url-opt-url {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-1);
}
.base-url-opt-name {
  flex: 0 0 auto;
  max-width: 40%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-3);
  font-size: 11.5px;
  text-align: right;
}
.base-url-manage {
  font-size: 12px;
  font-family: var(--font-ui);
  color: var(--text-3);
}
:global(.cs-pop.base-url-pop .cs-opt:hover .base-url-manage),
:global(.cs-pop.base-url-pop .cs-opt.hl .base-url-manage) {
  color: var(--text-2);
}
:global(.cs-pop.base-url-pop .cs-opt:last-child) {
  margin-top: 4px;
  padding-top: 8px;
  border-top: 1px solid var(--border);
  border-radius: 0 0 var(--radius-sm) var(--radius-sm);
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
/* 发送中按钮：转圈 + 计时 + 呼吸脉冲（计时只在此处显示）。 */
.bar-send.is-sending {
  min-width: 168px;
  animation: bar-send-pulse 1.4s ease-in-out infinite;
}
.bar-send-elapsed {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}
.btn-spinner {
  width: 13px;
  height: 13px;
  flex-shrink: 0;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.35);
  border-top-color: #fff;
  animation: btn-spin 0.7s linear infinite;
}
@keyframes btn-spin {
  to { transform: rotate(360deg); }
}
@keyframes bar-send-pulse {
  0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--danger) 45%, transparent); }
  50% { box-shadow: 0 0 0 7px transparent; }
}

/* 请求栏发送中：顶部流光 + 边框高亮（纯动画，无新增展示块）。 */
.request-bar {
  position: relative;
}
.request-bar.is-sending {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-tint);
}
.req-progress {
  position: absolute;
  left: 0;
  right: 0;
  top: 0;
  height: 2px;
  overflow: hidden;
  background: transparent;
  z-index: 2;
}
.req-progress::after {
  content: '';
  position: absolute;
  top: 0;
  bottom: 0;
  width: 40%;
  border-radius: 999px;
  background: linear-gradient(90deg, transparent, var(--accent), transparent);
  box-shadow: 0 0 8px var(--accent);
  animation: req-slide 0.9s ease-in-out infinite;
}
@keyframes req-slide {
  from { left: -40%; }
  to { left: 100%; }
}

/* Apifox 式请求中占位：大转圈 + 跳动计时 + 骨架 shimmer。 */
.req-loading {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 36px 20px 28px;
  border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
  border-radius: var(--radius);
  background: var(--bg-card);
  text-align: center;
}
.req-loading-ring {
  width: 30px;
  height: 30px;
  border-radius: 50%;
  border: 3px solid color-mix(in srgb, var(--accent) 25%, transparent);
  border-top-color: var(--accent);
  animation: btn-spin 0.7s linear infinite;
}
.req-loading-title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-1);
}
.req-loading-sub {
  margin: 0;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--text-3);
}
.req-loading-skel {
  width: 100%;
  margin-top: 6px;
  opacity: 0.85;
}
.req-loading-cancel {
  margin-top: 4px;
}

/* 响应体动画钩子：仅过渡 + 到达闪光，不新增展示块。 */
.response-anim {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  border-radius: var(--radius);
  transition:
    opacity var(--dur) var(--ease),
    filter var(--dur) var(--ease);
}
/* 新请求在途时旧响应降 dim（过渡动画，告知「正在刷新」。 */
.response-anim.is-stale {
  opacity: 0.55;
  filter: saturate(0.7);
}
/* 新响应到达：面板光晕闪光 + 状态徽章弹跳（Apifox 式到达感）。 */
.response-anim.flash {
  animation: resp-flash 1.1s ease-out;
}
.response-anim.flash :deep(.rp-status) {
  animation: status-pop 0.55s ease-out;
}
@keyframes resp-flash {
  0% {
    box-shadow:
      0 0 0 2px var(--accent),
      0 0 28px var(--accent-tint);
    background: var(--accent-tint);
  }
  60% { box-shadow: 0 0 0 1px var(--accent); }
  100% { box-shadow: 0 0 0 0 transparent; background: transparent; }
}
@keyframes status-pop {
  0% { transform: scale(0.6); }
  55% { transform: scale(1.14); }
  100% { transform: scale(1); }
}
@media (prefers-reduced-motion: reduce) {
  .bar-send.is-sending,
  .btn-spinner,
  .req-progress::after,
  .req-loading-ring,
  .response-anim.flash,
  .response-anim.flash :deep(.rp-status) {
    animation: none;
  }
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

/* ---- 配置区底部：单请求超时 / 跟随重定向（请求设置行） ---- */
.req-settings {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-wrap: wrap;
  padding-top: 6px;
  border-top: 1px dashed var(--border);
}
.rs-label {
  font-size: var(--fs-xs);
  color: var(--text-3);
}
.rs-field {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.rs-text {
  font-size: var(--fs-xs);
  color: var(--text-2);
}
.rs-timeout {
  width: 96px;
}
.rs-unit {
  font-size: var(--fs-xxs);
  color: var(--text-3);
}
.rs-check {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: var(--fs-xs);
  color: var(--text-2);
  cursor: pointer;
  user-select: none;
}
.rs-check input {
  accent-color: var(--accent);
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
.rp-splitter:focus-visible {
  outline: 2px solid var(--focus-ring);
  outline-offset: 2px;
  border-radius: var(--radius-sm);
}
.rp-splitter:focus-visible::before {
  background: var(--accent);
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
  font-size: var(--fs-xxs);
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
  font-size: var(--fs-xxs);
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