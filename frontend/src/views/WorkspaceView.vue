<script setup lang="ts">
/**
 * WorkspaceView：工作区主视图。
 * 左侧接口树（文件夹 + 接口，含 CRUD），右侧标签页 + 编辑器 + 响应区。
 * 树操作全部走 workspace store（Pinia），点击接口打开草稿标签页。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useWorkspaceStore } from '../stores/workspace'
import { useFoxApi } from '../composables/useFoxApi'
import { useToast } from '../composables/useToast'
import Brand from '../components/Brand.vue'
import ProjectTabs from '../components/ProjectTabs.vue'
import EndpointTree from '../components/EndpointTree.vue'
import EnvironmentBar from '../components/EnvironmentBar.vue'
import CookiePanel from '../components/CookiePanel.vue'
import HistoryPanel from '../components/HistoryPanel.vue'
import Icon from '../components/ui/Icon.vue'
import IconButton from '../components/ui/IconButton.vue'
import Menu, { type MenuItem } from '../components/ui/Menu.vue'
import Modal from '../components/ui/Modal.vue'
import SettingsDialog from '../components/SettingsDialog.vue'
import ShortcutsHelp from '../components/ShortcutsHelp.vue'
import TabBar from '../components/TabBar.vue'
import Tabs, { type TabItem } from '../components/ui/Tabs.vue'
import Tooltip from '../components/ui/Tooltip.vue'
import EndpointEditor from '../components/EndpointEditor.vue'
import CurlImportDialog from '../components/CurlImportDialog.vue'
import ImportDialog from '../components/ImportDialog.vue'
import MockRuleDialog from '../components/MockRuleDialog.vue'
import { useShortcuts } from '../composables/useShortcuts'
import { useWindowDrag } from '../composables/useWindowDrag'

const store = useWorkspaceStore()
const router = useRouter()
const api = useFoxApi()
const toast = useToast()

const loading = ref(false)
const showCurlImport = ref(false)
const curlFolderId = ref<string | null>(null)
const showDocImport = ref(false)
const showMockRules = ref(false)
const showSettings = ref(false)
const showShortcuts = ref(false)

/** 快捷键帮助（Ctrl+/）：集中注册表驱动，列表自动生成。 */
useShortcuts([
  {
    id: 'workspace.shortcuts-help',
    key: '/',
    group: '通用',
    description: '打开快捷键帮助',
    handler: () => {
      showShortcuts.value = true
    },
  },
])

/**
 * 侧栏搜索：输入即时回显（v-model），过滤词经 200ms 防抖下发。
 * EndpointTree 每个文件夹实例都会对全量 endpoints 做过滤，逐键触发
 * 会造成 O(文件夹数 × 接口数) 的重复计算风暴。
 */
const apiSearchInput = ref('')
const apiSearch = ref('')
let apiSearchTimer: ReturnType<typeof setTimeout> | undefined
watch(apiSearchInput, (v) => {
  if (apiSearchTimer) clearTimeout(apiSearchTimer)
  if (!v) {
    apiSearch.value = ''
    return
  }
  apiSearchTimer = setTimeout(() => {
    apiSearch.value = v
  }, 200)
})

function clearApiSearch(): void {
  if (apiSearchTimer) clearTimeout(apiSearchTimer)
  apiSearchInput.value = ''
  apiSearch.value = ''
}

// ---------- 侧栏宽度分割（拖拽调整，双击恢复默认） ----------
const SIDEBAR_MIN = 220
const SIDEBAR_MAX = 520
const SIDEBAR_DEFAULT = 300

const sidebarWidth = ref(SIDEBAR_DEFAULT)
const sidebarResizing = ref(false)
let sidebarDragging = false
let resizeStartX = 0
let resizeStartWidth = 0

function onSidebarResizeDown(event: MouseEvent): void {
  if (event.button !== 0) return
  event.preventDefault()
  sidebarDragging = true
  sidebarResizing.value = true
  resizeStartX = event.clientX
  resizeStartWidth = sidebarWidth.value
  document.body.style.userSelect = 'none'
  document.addEventListener('mousemove', onSidebarResizeMove)
  document.addEventListener('mouseup', onSidebarResizeUp)
}

function onSidebarResizeMove(event: MouseEvent): void {
  if (!sidebarDragging) return
  const next = resizeStartWidth + (event.clientX - resizeStartX)
  sidebarWidth.value = Math.min(Math.max(next, SIDEBAR_MIN), SIDEBAR_MAX)
}

function onSidebarResizeUp(): void {
  if (!sidebarDragging) return
  sidebarDragging = false
  sidebarResizing.value = false
  document.body.style.userSelect = ''
  document.removeEventListener('mousemove', onSidebarResizeMove)
  document.removeEventListener('mouseup', onSidebarResizeUp)
}

// ---------- 侧栏页签：接口目录 / 请求历史 ----------
type SidebarTab = 'collections' | 'history' | 'cookies'
const sidebarTab = ref<SidebarTab>('collections')
const sidebarTabs = computed<TabItem[]>(() => [
  { key: 'collections', label: '接口目录' },
  { key: 'history', label: '请求历史', count: store.histories.length || undefined },
  { key: 'cookies', label: 'Cookie' },
])

// ---------- Mock 服务 ----------
const mockAddress = ref<string | null>(null)
const mockBusy = ref(false)

async function refreshMockStatus(): Promise<void> {
  try {
    mockAddress.value = (await api.mockStatus()) ?? null
  } catch {
    mockAddress.value = null
  }
}

async function toggleMock(): Promise<void> {
  if (mockBusy.value) return
  mockBusy.value = true
  try {
    if (mockAddress.value) {
      await api.mockStop()
      mockAddress.value = null
      toast.success('Mock 服务已停止')
    } else {
      mockAddress.value = await api.mockStart()
      toast.success(`Mock 服务已启动：${mockAddress.value}`)
    }
  } catch (err) {
    toast.error('Mock 服务操作失败', { message: err instanceof Error ? err.message : String(err) })
  } finally {
    mockBusy.value = false
  }
}

async function load(): Promise<void> {
  loading.value = true
  try {
    if (!store.project) {
      const p = await store.init()
      if (!p) {
        router.replace('/projects')
        return
      }
    } else {
      await store.refresh()
    }
  } catch {
    // loadError 已在 store 内写入，界面展示重试
  } finally {
    loading.value = false
  }
}

async function exportOpenapi(): Promise<void> {
  if (!store.project) return
  try {
    const text = await api.exportOpenapi(store.project.id)
    const blob = new Blob([text], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `${store.project.name}-openapi.json`
    a.click()
    URL.revokeObjectURL(url)
    toast.success('已导出 OpenAPI 3.0 JSON')
  } catch (err) {
    toast.error('导出失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

function openCurlImport(folderId: string | null): void {
  curlFolderId.value = folderId
  showCurlImport.value = true
}

// ---------- 顶栏下拉菜单（文档 / Mock） ----------
const docsBtn = ref<HTMLButtonElement | null>(null)
const mockBtn = ref<HTMLButtonElement | null>(null)
const menuRef = ref<InstanceType<typeof Menu> | null>(null)

// ---------- 项目标签（顶栏多项目切换，共享组件） ----------
const treeRef = ref<InstanceType<typeof EndpointTree> | null>(null)

// ---------- 项目创建 / 重命名 / 删除 ----------
const showCreateProject = ref(false)
const newProjectName = ref('')
const newProjectDesc = ref('')
const projectFormError = ref<string | null>(null)

function openCreateProject(): void {
  newProjectName.value = ''
  newProjectDesc.value = ''
  projectFormError.value = null
  showCreateProject.value = true
}

async function confirmCreateProject(): Promise<void> {
  const name = newProjectName.value.trim()
  if (!name) {
    projectFormError.value = '项目名称不能为空'
    return
  }
  const now = new Date().toISOString()
  try {
    const p = await api.saveProject({
      id: crypto.randomUUID(),
      name,
      description: newProjectDesc.value.trim(),
      variables: {},
      created_at: now,
      updated_at: now,
    })
    showCreateProject.value = false
    await store.switchProject(p.id)
    toast.success('项目创建成功', { message: name })
  } catch (err) {
    toast.error('创建项目失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

const renamingProject = ref(false)
const renameProjectName = ref('')

function openRenameProject(): void {
  renameProjectName.value = store.project?.name ?? ''
  projectFormError.value = null
  renamingProject.value = true
}

async function confirmRenameProject(): Promise<void> {
  const name = renameProjectName.value.trim()
  if (!name) {
    projectFormError.value = '项目名称不能为空'
    return
  }
  const current = store.project
  if (!current) return
  try {
    const saved = await api.saveProject({ ...current, name, updated_at: new Date().toISOString() })
    store.project = saved
    renamingProject.value = false
    toast.success('项目已重命名')
  } catch (err) {
    toast.error('重命名失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

async function confirmDeleteProject(): Promise<void> {
  const target = store.project
  if (!target) return
  try {
    await api.deleteProject(target.id)
    await api.setActiveProject(null).catch(() => undefined)
    store.closeProjectTab(target.id)
    toast.success('项目已删除', { message: target.name })
    router.replace('/projects')
  } catch (err) {
    toast.error('删除项目失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

function createFolderAtRoot(): void {
  treeRef.value?.startEdit('create-folder', { parentId: null })
}

// ---------- 侧栏目录工具栏：「+ 新建」下拉 + 全部展开 / 折叠 ----------
const addBtn = ref<HTMLElement | null>(null)
const expandTick = ref(0)
const collapseTick = ref(0)

const CREATE_ITEMS: MenuItem[] = [
  { key: 'new-request', label: '新建 API 请求', icon: 'file-plus', iconAccent: true, shortcut: '⌘N' },
  { key: 'new-folder', label: '新建文件夹', icon: 'folder-plus' },
  { key: 'import-curl', label: '导入 cURL', icon: 'terminal', dividerBefore: true },
  { key: 'import-doc', label: '导入接口 (Postman / Swagger)', icon: 'upload' },
]

function openCreateMenu(): void {
  if (addBtn.value) menuRef.value?.openAt(addBtn.value, CREATE_ITEMS, 'right')
}

const DOCS_ITEMS: MenuItem[] = [
  { key: 'import', label: '导入文档', icon: 'upload' },
  { key: 'export', label: '导出 OpenAPI', icon: 'download' },
]

function openDocsMenu(): void {
  if (docsBtn.value) menuRef.value?.openAt(docsBtn.value, DOCS_ITEMS, 'right')
}

function onDocsSelect(item: MenuItem): void {
  if (item.key === 'import') showDocImport.value = true
  else void exportOpenapi()
}

function openMockMenu(): void {
  if (!mockBtn.value) return
  menuRef.value?.openAt(mockBtn.value, [
    {
      key: 'toggle',
      label: mockBusy.value ? '处理中…' : mockAddress.value ? '停止 Mock' : '启动 Mock',
      icon: mockAddress.value ? 'stop' : 'play',
      disabled: mockBusy.value,
    },
    { key: 'rules', label: 'Mock 规则', icon: 'shield', dividerBefore: true },
  ], 'right')
}

function onMockSelect(item: MenuItem): void {
  if (item.key === 'toggle') void toggleMock()
  else if (item.key === 'rules') showMockRules.value = true
}

function onMenuSelect(item: MenuItem): void {
  if (item.key === 'new-project') openCreateProject()
  else if (item.key === 'import' || item.key === 'export') onDocsSelect(item)
  else if (item.key === 'new-request') store.openNewEndpoint(null)
  else if (item.key === 'new-folder') createFolderAtRoot()
  else if (item.key === 'import-curl') openCurlImport(null)
  else if (item.key === 'import-doc') showDocImport.value = true
  else onMockSelect(item)
}

/** Agent 控制面事件载荷（fox-agent::server::AgentEvent，字段 snake_case）。 */
interface AgentEventPayload {
  type: string
  endpoint_id?: string
  project_id?: string
  name?: string
}

let unlistenAgent: UnlistenFn | null = null

// 关闭最后一个项目标签（或项目被删）后回到项目列表
watch(
  () => store.project,
  (p) => {
    if (!p) void router.replace('/projects')
  },
)

onMounted(() => {
  load()
  refreshMockStatus()
  void store.loadHistories()
  // AI Agent 经控制面导入接口后刷新侧栏并提示（仅当前激活项目时）
  if ('__TAURI_INTERNALS__' in window) {
    void listen<AgentEventPayload>('fox:agent-event', async (event) => {
      const payload = event.payload
      if (payload.type !== 'endpoint-imported') return
      if (!store.project || payload.project_id !== store.project.id) return
      await store.refresh()
      toast.info(`AI Agent 已导入接口「${payload.name ?? '未命名'}」`)
    }).then((unlisten) => {
      unlistenAgent = unlisten
    })
  }
})

/** 顶栏拖拽窗口：空白处 mousedown → startDragging；交互元素（按钮/输入等）跳过；双击切换最大化。 */
const topBarEl = ref<HTMLElement | null>(null)
useWindowDrag(topBarEl)

onBeforeUnmount(() => {
  menuRef.value?.close()
  unlistenAgent?.()
  if (apiSearchTimer) clearTimeout(apiSearchTimer)
  document.removeEventListener('mousemove', onSidebarResizeMove)
  document.removeEventListener('mouseup', onSidebarResizeUp)
})
</script>

<template>
  <div class="workspace">
    <div v-if="store.project" ref="topBarEl" class="top-bar">
      <div class="tb-region tb-left">
        <Brand title="RustFox" class="tb-brand" />
      </div>
      <span class="tb-divider" aria-hidden="true"></span>

      <div class="tb-region tb-projects">
        <ProjectTabs
          project-actions
          @new-project="openCreateProject"
          @rename-project="openRenameProject"
          @delete-project="confirmDeleteProject"
        />
      </div>
      <span class="tb-divider" aria-hidden="true"></span>

      <div class="tb-region tb-right">
        <button ref="docsBtn" class="rf-btn rf-btn-sm tb-action" type="button" @click="openDocsMenu">
          <Icon name="file" :size="13" /> 文档 (Docs) <Icon name="chevron-down" :size="12" />
        </button>
        <button ref="mockBtn" class="rf-btn rf-btn-sm tb-action" type="button" @click="openMockMenu">
          <span class="mock-dot" :class="{ on: mockAddress }"></span>
          Mock <Icon name="chevron-down" :size="12" />
        </button>
        <EnvironmentBar />
        <IconButton
          class="tb-settings"
          name="code"
          :size="15"
          title="GraphQL 工作台"
          @click="router.push('/graphql')"
        />
        <IconButton
          class="tb-settings"
          name="zap"
          :size="15"
          title="实时调试（WebSocket / SSE）"
          @click="router.push('/realtime')"
        />
        <IconButton
          class="tb-settings"
          name="keyboard"
          :size="15"
          title="快捷键（Ctrl+/）"
          @click="showShortcuts = true"
        />
        <IconButton class="tb-settings" name="settings" :size="15" title="设置" @click="showSettings = true" />
      </div>
    </div>

    <div class="workspace-body">
      <aside class="rf-sidebar" :style="{ width: `${sidebarWidth}px` }">
        <Tabs v-model="sidebarTab" :tabs="sidebarTabs" size="sm" class="sidebar-tabs" />
        <div v-show="sidebarTab === 'collections'" class="sidebar-collections">
          <div class="sidebar-toolbar">
            <div class="sidebar-search">
              <Icon name="search" :size="13" class="ss-icon" />
              <input
                v-model="apiSearchInput"
                class="ss-input"
                type="text"
                placeholder="搜索接口名称或路径..."
                spellcheck="false"
              />
              <button
                v-if="apiSearchInput"
                class="ss-clear"
                type="button"
                title="清除搜索"
                aria-label="清除搜索"
                @click="clearApiSearch"
              >
                <Icon name="x" :size="12" />
              </button>
            </div>
            <div class="sidebar-tools">
              <Tooltip content="新建（⌘N）">
                <button
                  ref="addBtn"
                  class="tool-add"
                  type="button"
                  aria-label="新建接口 / 文件夹 / 导入"
                  @click="openCreateMenu"
                >
                  <Icon name="plus" :size="14" />
                </button>
              </Tooltip>
              <Tooltip content="全部折叠">
                <IconButton name="chevrons-down-up" :size="14" @click="collapseTick++" />
              </Tooltip>
              <Tooltip content="全部展开">
                <IconButton name="chevrons-up-down" :size="14" @click="expandTick++" />
              </Tooltip>
            </div>
          </div>
          <div v-if="store.loadError" class="rf-inline-error" role="alert">
            <span class="rf-inline-error-text">加载失败：{{ store.loadError }}</span>
            <button class="rf-btn rf-btn-sm" type="button" :disabled="loading" @click="load">
              {{ loading ? '重试中…' : '重试' }}
            </button>
          </div>
          <div v-else class="tree-wrap">
            <EndpointTree
              ref="treeRef"
              :folder-id="null"
              :search="apiSearch"
              :expand-tick="expandTick"
              :collapse-tick="collapseTick"
              @import-curl="openCurlImport"
            />
          </div>
        </div>
        <HistoryPanel v-if="sidebarTab === 'history'" class="sidebar-history" />
        <CookiePanel v-if="sidebarTab === 'cookies'" class="sidebar-history" />
      </aside>
      <div
        class="sidebar-resizer"
        :class="{ active: sidebarResizing }"
        role="separator"
        aria-orientation="vertical"
        aria-label="调整侧栏宽度"
        title="拖拽调整宽度，双击恢复默认"
        @mousedown="onSidebarResizeDown"
        @dblclick="sidebarWidth = 300"
      ></div>
      <main class="rf-main">
        <TabBar
          v-if="store.openTabs.length"
          @import-curl="openCurlImport(null)"
          @import-openapi="showDocImport = true"
        />
        <EndpointEditor />
      </main>
    </div>

    <Menu ref="menuRef" @select="onMenuSelect" />

    <Modal v-model:open="showCreateProject" title="新建项目" width="420px" @close="showCreateProject = false">
      <div class="form-field">
        <label class="form-label">项目名称</label>
        <input
          v-model="newProjectName"
          class="rf-input"
          :class="{ 'rf-input-error': projectFormError }"
          placeholder="例如：电子商务后端 API"
          maxlength="60"
          spellcheck="false"
          @input="projectFormError = null"
          @keyup.enter="confirmCreateProject"
        />
      </div>
      <div class="form-field">
        <label class="form-label">描述（可选）</label>
        <textarea
          v-model="newProjectDesc"
          class="rf-textarea"
          :maxlength="200"
          placeholder="项目用途与说明…"
          rows="3"
        ></textarea>
      </div>
      <p v-if="projectFormError" class="rf-field-error" role="alert">{{ projectFormError }}</p>
      <template #footer>
        <button class="rf-btn" type="button" @click="showCreateProject = false">取消</button>
        <button class="rf-btn rf-btn-primary" type="button" :disabled="api.pending.value" @click="confirmCreateProject">
          创建
        </button>
      </template>
    </Modal>

    <Modal v-model:open="renamingProject" title="重命名项目" width="420px" @close="renamingProject = false">
      <div class="form-field">
        <label class="form-label">项目名称</label>
        <input
          v-model="renameProjectName"
          class="rf-input"
          v-focus-end
          :class="{ 'rf-input-error': projectFormError }"
          placeholder="项目名称"
          maxlength="60"
          spellcheck="false"
          @input="projectFormError = null"
          @keyup.enter="confirmRenameProject"
        />
      </div>
      <p v-if="projectFormError" class="rf-field-error" role="alert">{{ projectFormError }}</p>
      <template #footer>
        <button class="rf-btn" type="button" @click="renamingProject = false">取消</button>
        <button class="rf-btn rf-btn-primary" type="button" :disabled="api.pending.value" @click="confirmRenameProject">
          保存
        </button>
      </template>
    </Modal>

    <CurlImportDialog
      v-if="showCurlImport"
      :folder-id="curlFolderId"
      @close="showCurlImport = false"
    />
    <ImportDialog v-if="showDocImport" @close="showDocImport = false" />
    <MockRuleDialog v-if="showMockRules" @close="showMockRules = false" />
    <SettingsDialog v-if="showSettings" @close="showSettings = false" />
    <ShortcutsHelp :open="showShortcuts" @update:open="showShortcuts = $event" />
  </div>
</template>

<style scoped>
.workspace {
  display: flex;
  flex-direction: column;
  height: 100%;
  box-sizing: border-box;
}

.workspace-body {
  flex: 1;
  display: flex;
  min-height: 0;
}

/* ---- 顶栏：左（品牌）/ 中（状态）/ 右（操作 + 环境）三区 ---- */
.top-bar {
  display: flex;
  align-items: center;
  height: 48px;
  padding: 0 12px;
  gap: 12px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--rf-border);
  background: var(--rf-bg-panel);
  cursor: grab;
  user-select: none;
}

.tb-region {
  display: flex;
  align-items: center;
  min-width: 0;
}

.tb-left {
  flex: 0 1 auto;
}

.tb-brand {
  width: 140px;
}

/* ---- 顶栏项目标签条（ProjectTabs 组件） ---- */
.tb-projects {
  flex: 1 1 auto;
  gap: 6px;
  overflow: hidden;
}

.tb-right {
  margin-left: auto;
  justify-content: flex-end;
  gap: 8px;
  flex: 1;
}

.tb-divider {
  width: 1px;
  height: 20px;
  flex-shrink: 0;
  background: var(--rf-border);
  opacity: 0.55;
}

.tb-inner-divider {
  width: 1px;
  height: 20px;
  flex-shrink: 0;
  margin: 0 2px;
  background: var(--rf-border);
  opacity: 0.55;
}

/* ---- 顶栏右区操作：统一 h-8、border-white/10、rounded-lg、gap-2 ---- */
.tb-right .tb-action {
  height: 32px;
  padding: 0 12px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  background: var(--bg-hover);
  font-size: 12px;
  gap: 6px;
  color: var(--text-2);
}
.tb-right .tb-action:hover:not(:disabled) {
  background: var(--bg-active);
  border-color: rgba(255, 255, 255, 0.2);
  color: var(--text-1);
}
.tb-right .tb-settings {
  width: 32px;
  height: 32px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  background: var(--bg-hover);
}
.tb-right .tb-settings:hover:not(:disabled) {
  background: var(--bg-active);
}

/* ---- Mock 状态圆点（按钮内指示） ---- */
.mock-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
  background: var(--rf-text-muted);
}

.mock-dot.on {
  background: var(--rf-success);
  box-shadow: 0 0 0 3px var(--rf-success-tint);
}

.rf-sidebar {
  /* 宽度由内联 style 驱动（拖拽分割条），保留 min/max 约束兜底 */
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  border-right: 1px solid var(--rf-border);
  background: var(--rf-bg-panel);
  overflow: hidden;
}

/* ---- 拖拽分割条：透明叠加在边框上，悬停/拖拽中高亮 ---- */
.sidebar-resizer {
  flex-shrink: 0;
  width: 7px;
  margin-left: -4px;
  cursor: col-resize;
  z-index: 10;
}
.sidebar-resizer:hover,
.sidebar-resizer.active {
  background: var(--accent);
}

.rf-heading {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ---- 目录工具栏：搜索框占满 + 右侧「+ 新建」主按钮 / 折叠展开 ---- */
.sidebar-toolbar {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 8px 12px 0;
}

.sidebar-toolbar .sidebar-search {
  flex: 1;
  min-width: 0;
  margin: 0;
}

.sidebar-tools {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

/* 主操作「+」：accent 微光方块，页面侧栏唯一强调按钮 */
.tool-add {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
  border-radius: 7px;
  background: var(--accent-tint);
  color: var(--accent);
  cursor: pointer;
  transition:
    background var(--dur) var(--ease),
    border-color var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease),
    color var(--dur) var(--ease);
}
.tool-add:hover {
  border-color: color-mix(in srgb, var(--accent) 85%, transparent);
  background: color-mix(in srgb, var(--accent) 26%, transparent);
  color: var(--accent-hover);
}
.tool-add:active {
  transform: translateY(1px);
}

/* ---- 接口搜索栏：h-7、bg-white/5、rounded、px-2.5、text-xs、text-gray-300 */
.sidebar-search {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  margin: 8px 12px 0;
  padding: 0 8px;
  flex-shrink: 0;
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.05);
  transition:
    border-color var(--dur) var(--ease),
    background var(--dur) var(--ease);
}
.sidebar-search:focus-within {
  border-color: color-mix(in srgb, var(--accent) 50%, transparent);
  background: rgba(255, 255, 255, 0.07);
}

.ss-icon {
  flex-shrink: 0;
  color: var(--text-3);
  opacity: 0.7;
}

.ss-input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font-family: inherit;
  font-size: 12px;
  color: #d1d5db;
}
.ss-input::placeholder {
  color: rgba(209, 213, 219, 0.45);
}

.ss-clear {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 4px;
  background: none;
  color: var(--text-3);
  cursor: pointer;
  padding: 0;
  transition:
    background var(--dur) var(--ease),
    color var(--dur) var(--ease);
}
.ss-clear:hover {
  background: var(--bg-hover);
  color: var(--text-1);
}

.tree-wrap {
  flex: 1;
  overflow-y: auto;
  padding: 8px 12px 12px;
}

/* ---- 侧栏页签（接口目录 / 请求历史） ---- */
.sidebar-tabs {
  flex-shrink: 0;
  padding: 0 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}

.sidebar-collections {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.sidebar-history {
  flex: 1;
  min-height: 0;
}

.rf-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
  background: var(--rf-bg);
  overflow: hidden;
}

.rf-inline-error {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 10px 12px;
  border-radius: var(--rf-radius-sm);
  background: var(--rf-danger-tint);
  border: 1px solid rgba(239, 68, 68, 0.35);
}

.rf-inline-error-text {
  font-size: 12.5px;
  color: var(--rf-danger);
  word-break: break-all;
}

/* ---- 项目表单（新建 / 重命名） ---- */
.form-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 12px;
}

.form-label {
  font-size: 12px;
  color: var(--text-2);
}

.rf-input-error {
  border-color: var(--danger) !important;
}
.rf-input-error:focus {
  border-color: var(--danger) !important;
  box-shadow: 0 0 0 2px var(--danger-tint) !important;
}
</style>