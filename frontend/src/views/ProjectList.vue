<script setup lang="ts">
/**
 * ProjectList：仪表板视图（Dashboard）。
 *
 * - 顶部栏：RustFox 品牌 + 设置（无重复搜索框）；
 * - 左侧导航：DashboardNav 组件；
 * - 摘要区：总 API 数 + 最近修改项目 + 快速请求入口；
 * - 工具栏：名称过滤 + 视图切换（网格/列表）+ 排序（最近修改/名称/API数量）+ 新建项目；
 * - 项目卡片 / 弹窗（新建、重命名、删除、快速请求）拆分至 components/projectlist/。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { ensureSwapMounted } from '../utils/sortable'
import Sortable from 'sortablejs'
import { useFoxApi } from '../composables/useFoxApi'
import { useToast } from '../composables/useToast'
import { useWorkspaceStore } from '../stores/workspace'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import Icon from '../components/ui/Icon.vue'
import IconButton from '../components/ui/IconButton.vue'
import SettingsDialog from '../components/SettingsDialog.vue'
import DashboardNav from '../components/projectlist/DashboardNav.vue'
import ProjectTabs from '../components/ProjectTabs.vue'
import ImportDialog from '../components/ImportDialog.vue'
import ProjectCard from '../components/projectlist/ProjectCard.vue'
import ProjectCreateModal from '../components/projectlist/ProjectCreateModal.vue'
import ProjectRenameModal from '../components/projectlist/ProjectRenameModal.vue'
import ProjectDeleteModal from '../components/projectlist/ProjectDeleteModal.vue'
import ScratchRequestModal from '../components/projectlist/ScratchRequestModal.vue'
import { timeAgo } from '../components/projectlist/projectMeta'
import { aggregateProjectStats, totalEndpointCount } from '../utils/projectStats'
import { useWindowDrag } from '../composables/useWindowDrag'
import logo from '../assets/rustfox-logo.png'
import type { HttpMethod, Project } from '../types/foxApi'

const api = useFoxApi()
const toast = useToast()
const router = useRouter()
const workspace = useWorkspaceStore()

const projects = ref<Project[]>([])
const counts = ref<Record<string, number>>({})
/** 每个项目最近更新的接口摘要（仅 method/path，统计命令返回，不拉全量）。 */
const latestEndpoints = ref<Record<string, { method: HttpMethod; path: string } | null>>({})
const loading = ref(false)
const loadError = ref<string | null>(null)
const search = ref('')

// ---------- 摘要 ----------
const totalProjects = computed(() => projects.value.length)

const totalApis = computed(() => totalEndpointCount(counts.value))

/** 按时段问候（Hero 欢迎语）。 */
const greeting = computed(() => {
  const h = new Date().getHours()
  if (h < 5) return '夜深了'
  if (h < 11) return '早上好'
  if (h < 14) return '中午好'
  if (h < 18) return '下午好'
  return '晚上好'
})

/** 最近活动条目的 Method Badge（GraphQL 端点单独识别为粉色 GQL）。 */
function badgeOf(projectId: string): { text: string; cls: string } {
  const ep = latestEndpoints.value[projectId]
  if (!ep) return { text: 'NEW', cls: 'new' }
  if (/graphql/i.test(ep.path)) return { text: 'GQL', cls: 'gql' }
  const cls = ['get', 'post', 'put', 'delete', 'patch'].includes(ep.method.toLowerCase())
    ? ep.method.toLowerCase()
    : 'other'
  return { text: ep.method === 'DELETE' ? 'DEL' : ep.method, cls }
}

const recentProjects = computed(() =>
  [...projects.value].sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, 3),
)

// ---------- 过滤 ----------
const filtered = computed(() => {
  const q = search.value.trim().toLowerCase()
  // 无排序控件：保持数据库顺序（即用户手动拖拽的持久化顺序）
  if (!q) return [...projects.value]
  return projects.value.filter((p) => p.name.toLowerCase().includes(q))
})

// ---------- 拖拽排序 ----------
/** 手动顺序 + 无搜索过滤时才允许拖拽（否则顺序无意义）。 */
const dragEnabled = computed(
  () => !search.value.trim() && projects.value.length > 1,
)

const gridEl = ref<HTMLElement | null>(null)
const sortable = ref<Sortable | null>(null)
const savingOrder = ref(false)

/** 交换式拖拽：A 拖到 B 后面时 B 直接让位到 A 原位，无 ghost 空白占位。 */
ensureSwapMounted()

/** 拖拽结束后的短暂窗口内抑制卡片 click（防止拖完被带进项目）。 */
let suppressClickUntil = 0

/** 按当前 DOM 顺序重排 projects 并持久化。 */
async function onDragEnd(): Promise<void> {
  suppressClickUntil = Date.now() + 400
  const ordered: Project[] = []
  const els = gridEl.value?.querySelectorAll<HTMLElement>('[data-project-id]') ?? []
  for (const el of els) {
    const found = projects.value.find((p) => p.id === el.dataset.projectId)
    if (found) ordered.push(found)
  }
  if (ordered.length !== projects.value.length) return
  projects.value = ordered
  if (savingOrder.value) return
  savingOrder.value = true
  try {
    await api.updateProjectsOrder(ordered.map((p) => p.id))
  } catch (e) {
    toast.error('排序保存失败', { message: e instanceof Error ? e.message : String(e), duration: 4000 })
  } finally {
    savingOrder.value = false
  }
}

/** 拖拽刚结束的 click 不进入项目。 */
function onCardOpen(p: Project): void {
  if (Date.now() < suppressClickUntil) return
  void enter(p)
}

function initSortable(): void {
  if (sortable.value || !gridEl.value) return
  console.log('[dnd] init sortable', gridEl.value)
  sortable.value = Sortable.create(gridEl.value, {
    animation: 200,
    easing: 'cubic-bezier(0.2, 0, 0, 1)',
    swap: true,
    swapClass: 'sortable-swap',
    swapThreshold: 0.75,
    forceFallback: true,
    fallbackClass: 'sortable-drag',
    fallbackOnBody: true,
    fallbackTolerance: 3,
    ghostClass: 'sortable-ghost',
    dragClass: 'sortable-drag',
    chosenClass: 'sortable-chosen',
    disabled: !dragEnabled.value,
    filter: 'button, a, input, select, textarea, [data-no-drag]',
    preventOnFilter: true,
    onChoose: () => console.log('[dnd] choose'),
    onStart: () => console.log('[dnd] start'),
    onEnd: () => {
      console.log('[dnd] end')
      void onDragEnd()
    },
    onMove: (evt) =>
      !evt.related?.closest('button, a, input, select, textarea, [data-no-drag]'),
  })
}

onMounted(initSortable)

/** 列表容器挂载/重建时初始化（v-if 延迟渲染，ref 变化是最可靠信号）。 */
watch(gridEl, (el) => {
  if (el && !sortable.value) initSortable()
})

watch(dragEnabled, (enabled) => {
  sortable.value?.option('disabled', !enabled)
})

onBeforeUnmount(() => {
  sortable.value?.destroy()
  sortable.value = null
})

function isActive(p: Project): boolean {
  return api.activeProject.value?.id === p.id
}

// ---------- 数据 ----------
async function loadCounts(): Promise<void> {
  // 单条统计 IPC（后端聚合），替代逐项目拉全量接口的 N+1 加载；
  // 字段校验与聚合在 aggregateProjectStats（snake_case 契约，畸形条目跳过）
  try {
    const stats = await api.listProjectStats()
    const { counts: nextCounts, latest: nextLatest } = aggregateProjectStats(stats)
    counts.value = nextCounts
    latestEndpoints.value = nextLatest
  } catch {
    counts.value = {}
    latestEndpoints.value = {}
  }
}

async function load(): Promise<void> {
  if (loading.value) return
  loading.value = true
  loadError.value = null
  try {
    projects.value = await api.getProjects()
    counts.value = {}
    await loadCounts()
  } catch (e) {
    loadError.value = e instanceof Error ? e.message : String(e)
    toast.error('项目列表加载失败', { message: loadError.value, duration: 6000 })
  } finally {
    loading.value = false
  }
}

onMounted(load)

// ---------- 新建 ----------
const showCreate = ref(false)

function onCreated(project: Project): void {
  projects.value.push(project)
  counts.value[project.id] = 0
}

// ---------- 进入工作区 ----------
async function enter(project: Project): Promise<void> {
  try {
    // 走 store 切换：当前项目 UI 态入快照，目标项目加入顶栏标签
    // （成功不弹 toast：跳转到工作区本身已是明确反馈）
    await workspace.switchProject(project.id)
    router.push('/workspace')
  } catch (e) {
    toast.error('进入项目失败', { message: e instanceof Error ? e.message : String(e), duration: 6000 })
  }
}

// ---------- 快速请求 / 导入 ----------
const showScratch = ref(false)
const showImport = ref(false)
/** 拖拽导入的预填文档内容（Dropzone 读入文件后带内容打开 ImportDialog）。 */
const droppedText = ref('')

/** 仪表板导入成功（新项目已创建并激活）：加入本地列表。 */
function onImported(project: Project): void {
  projects.value.push(project)
  counts.value[project.id] = 0
}

// ---------- 拖拽导入（Dropzone） ----------
// Tauri 拦截了 HTML5 drop 事件（dragDropEnabled），文件拖放只提供物理坐标 +
// 路径：监听 webview 拖拽事件，命中 Dropzone 区域时读取文件内容走导入弹窗。
const dropEl = ref<HTMLElement | null>(null)
const dropActive = ref(false)
let unlistenDrop: UnlistenFn | null = null

function inDropZone(px: number, py: number): boolean {
  const rect = dropEl.value?.getBoundingClientRect()
  if (!rect) return false
  // 事件坐标是物理像素，rect 是 CSS 像素
  const dpr = window.devicePixelRatio || 1
  const x = px / dpr
  const y = py / dpr
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}

async function importDroppedFile(path?: string): Promise<void> {
  if (!path) {
    toast.error('拖拽导入失败', { message: '未取到文件路径' })
    return
  }
  try {
    droppedText.value = await api.readTextFile(path)
    showImport.value = true
  } catch (e) {
    toast.error('读取文件失败', { message: e instanceof Error ? e.message : String(e), duration: 5000 })
  }
}

onMounted(async () => {
  if (!('__TAURI_INTERNALS__' in window)) return
  try {
    unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload
      if (payload.type === 'enter' || payload.type === 'over') {
        dropActive.value = inDropZone(payload.position.x, payload.position.y)
      } else if (payload.type === 'drop') {
        const inside = inDropZone(payload.position.x, payload.position.y)
        dropActive.value = false
        if (inside) void importDroppedFile(payload.paths[0])
      } else {
        dropActive.value = false
      }
    })
  } catch {
    // 拖拽事件不可用（如纯浏览器 dev）时静默降级为仅点击导入
  }
})

onBeforeUnmount(() => {
  unlistenDrop?.()
  unlistenDrop = null
})

// ---------- 卡片菜单：重命名 / 复制 / 删除 ----------
const menuOpenId = ref<string | null>(null)

function toggleMenu(id: string): void {
  menuOpenId.value = menuOpenId.value === id ? null : id
}

function closeMenu(): void {
  menuOpenId.value = null
}

function onDocClick(): void {
  closeMenu()
}

onMounted(() => document.addEventListener('click', onDocClick))
onBeforeUnmount(() => document.removeEventListener('click', onDocClick))

const renaming = ref<Project | null>(null)

function openRename(p: Project): void {
  closeMenu()
  renaming.value = p
}

function onRenamed(saved: Project): void {
  const idx = projects.value.findIndex((p) => p.id === saved.id)
  if (idx !== -1) projects.value[idx] = saved
}

async function duplicate(p: Project): Promise<void> {
  closeMenu()
  const now = new Date().toISOString()
  try {
    const copy = await api.saveProject({
      ...p,
      id: crypto.randomUUID(),
      name: `${p.name} 副本`,
      created_at: now,
      updated_at: now,
    })
    projects.value.push(copy)
    counts.value[copy.id] = 0
    toast.success('项目已复制', { message: copy.name })
  } catch (e) {
    toast.error('复制项目失败', { message: e instanceof Error ? e.message : String(e), duration: 6000 })
  }
}

const deleting = ref<Project | null>(null)

function openDelete(p: Project): void {
  closeMenu()
  deleting.value = p
}

async function onDeleted(id: string): Promise<void> {
  projects.value = projects.value.filter((p) => p.id !== id)
  delete counts.value[id]
  if (api.activeProject.value?.id === id) {
    await api.setActiveProject(null).catch(() => undefined)
  }
}

const showSettings = ref(false)

/** 顶栏拖拽窗口（与工作区顶栏共用 useWindowDrag）。 */
const topBarEl = ref<HTMLElement | null>(null)
useWindowDrag(topBarEl)
</script>

<template>
  <div class="dash">
    <header ref="topBarEl" class="dash-top">
      <button class="top-brand" type="button" title="回到项目首页" @click="router.push('/projects')">
        <span class="top-logo" aria-hidden="true">
          <img :src="logo" alt="" width="18" height="18" />
        </span>
        <span class="top-title">RustFox</span>
        <span class="top-tag">API 调试工具</span>
      </button>
      <ProjectTabs class="top-tabs" @new-project="showCreate = true" />
      <div class="top-right">
        <IconButton name="settings" :size="15" title="设置" @click="showSettings = true" />
      </div>
    </header>

    <div class="dash-body">
      <DashboardNav />

      <main class="dash-main">
        <div v-if="loadError" class="rf-inline-error" role="alert">
          <span class="rf-inline-error-text">加载失败：{{ loadError }}</span>
          <button class="rf-btn rf-btn-sm" type="button" :disabled="loading" @click="load">
            {{ loading ? '重试中…' : '重试' }}
          </button>
        </div>

        <template v-else>
          <section class="summary-grid">
            <!-- 卡片 1：Hero 欢迎语 + 渐变大数字统计 -->
            <div class="stat-card">
              <span class="stat-icon tint-indigo"><Icon name="gauge" :size="17" /></span>
              <div class="stat-body">
                <div class="hero-greet">
                  <p class="hero-title">{{ greeting }}，欢迎回来</p>
                  <p class="hero-sub">管理你的 API 项目与接口资产</p>
                </div>
                <div class="hero-stats">
                  <div class="hero-stat">
                    <span class="hero-value num">{{ totalProjects }}</span>
                    <span class="hero-label"><Icon name="folder" :size="12" /> 总项目数</span>
                  </div>
                  <span class="hero-divider" aria-hidden="true"></span>
                  <div class="hero-stat">
                    <span class="hero-value num">{{ totalApis }}</span>
                    <span class="hero-label"><Icon name="plug" :size="12" /> 总接口数</span>
                  </div>
                </div>
              </div>
            </div>

            <!-- 卡片 2：最近活动（时间线条目 + Method Badge） -->
            <div class="stat-card">
              <span class="stat-icon tint-violet"><Icon name="clock" :size="16" /></span>
              <div class="stat-body">
                <span class="stat-label">最近活动</span>
                <div v-if="recentProjects.length" class="timeline">
                  <button
                    v-for="p in recentProjects"
                    :key="p.id"
                    class="tl-item"
                    type="button"
                    @click="enter(p)"
                  >
                    <span class="tl-badge" :class="badgeOf(p.id).cls">{{ badgeOf(p.id).text }}</span>
                    <span class="tl-main">
                      <span class="tl-line">
                        <span class="tl-name">{{ p.name }}</span>
                        <span class="tl-time">{{ timeAgo(p.updated_at) }}</span>
                      </span>
                      <span v-if="latestEndpoints[p.id]" class="tl-path mono">
                        {{ latestEndpoints[p.id]!.method }} {{ latestEndpoints[p.id]!.path }}
                      </span>
                    </span>
                  </button>
                </div>
                <span v-else class="stat-sub">暂无活动</span>
              </div>
            </div>

            <!-- 卡片 3：快速开始（主 CTA 光晕按钮 + 次要导入） -->
            <div class="stat-card">
              <span class="stat-icon tint-amber"><Icon name="zap" :size="16" /></span>
              <div class="stat-body">
                <span class="stat-label">快速开始</span>
                <div class="quick-row">
                  <button class="quick-btn primary" type="button" title="发送临时不保存的请求" @click="showScratch = true">
                    <Icon name="send" :size="12" /> 快速请求
                  </button>
                  <button
                    class="quick-btn ghost"
                    type="button"
                    title="从 Postman / Swagger / OpenAPI 导入为新项目"
                    @click="droppedText = ''; showImport = true"
                  >
                    <Icon name="download" :size="12" /> 导入项目
                  </button>
                </div>
              </div>
            </div>
          </section>

          <section class="toolbar">
            <div class="toolbar-filter">
              <Icon name="search" :size="14" />
              <input
                v-model="search"
                class="toolbar-filter-input"
                placeholder="按名称过滤项目…"
                spellcheck="false"
              />
            </div>
            <button class="btn-new" type="button" @click="showCreate = true">
              <Icon name="plus" :size="15" /> 新建 API 项目
            </button>
          </section>

          <div
            v-if="filtered.length"
            ref="gridEl"
            class="card-grid"
          >
            <ProjectCard
              v-for="p in filtered"
              :key="p.id"
              :project="p"
              :count="counts[p.id] ?? 0"
              :active="isActive(p)"
              :menu-open="menuOpenId === p.id"
              :draggable="dragEnabled"
              @open="onCardOpen(p)"
              @toggle-menu="toggleMenu(p.id)"
              @rename="openRename(p)"
              @duplicate="duplicate(p)"
              @delete="openDelete(p)"
            />
          </div>

          <div v-else-if="loading" class="dash-empty">
            <p class="rf-hint">加载中…</p>
          </div>

          <!-- 底部 Dropzone：拖入文档直接导入，或点击按钮创建 / 选择文件 -->
          <div
            v-if="filtered.length && !loading"
            ref="dropEl"
            class="add-card"
            :class="{ 'drop-active': dropActive }"
            role="group"
            aria-label="快捷创建或导入项目"
          >
            <span class="add-icon">
              <Icon :name="dropActive ? 'download' : 'folder-plus'" :size="20" />
            </span>
            <p class="add-text">
              <template v-if="dropActive">松开鼠标，导入拖入的文档文件</template>
              <template v-else>
                拖入 Postman / Swagger / OpenAPI 文件即可导入，或
              </template>
            </p>
            <div class="add-actions">
              <button class="add-btn" type="button" @click="showCreate = true">
                <Icon name="plus" :size="13" /> 新建项目
              </button>
              <button class="add-btn" type="button" @click="droppedText = ''; showImport = true">
                <Icon name="download" :size="13" /> 导入外部文档
              </button>
            </div>
          </div>

          <div v-else-if="!filtered.length && !loading" class="dash-empty">
            <span class="empty-icon"><Icon name="folder" :size="30" /></span>
            <p class="empty-title">{{ search ? '没有匹配的项目' : '还没有项目' }}</p>
            <p class="empty-hint">
              {{ search ? '换个关键词试试，或创建一个新项目。' : '创建你的第一个 API 项目，开始设计、调试与 Mock。' }}
            </p>
            <button class="rf-btn rf-btn-primary" type="button" @click="showCreate = true">
              <Icon name="plus" :size="14" /> 新建 API 项目
            </button>
          </div>
        </template>
      </main>
    </div>

    <ProjectCreateModal v-model:open="showCreate" @created="onCreated" />

    <ProjectRenameModal :project="renaming" @close="renaming = null" @saved="onRenamed" />

    <ProjectDeleteModal :project="deleting" @close="deleting = null" @deleted="onDeleted" />

    <ScratchRequestModal v-model:open="showScratch" />

    <!-- 仪表板导入：创建新项目承接（区别于工作区内导入到当前项目）；
         拖拽导入时带文件内容直接打开并自动解析 -->
    <ImportDialog
      v-if="showImport"
      mode="new-project"
      :initial-text="droppedText"
      @imported="onImported"
      @close="showImport = false"
    />

    <SettingsDialog v-if="showSettings" @close="showSettings = false" />
  </div>
</template>

<style scoped>
.dash {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--bg-app);
}

/* ---------- 顶部栏 ---------- */
.dash-top {
  height: 56px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 0 16px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-panel);
  cursor: grab;
  user-select: none;
}

/* 标签条在顶栏内占满品牌与设置之间 */
.top-tabs {
  flex: 1 1 auto;
  min-width: 0;
}

.top-brand {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
  padding: 4px 8px;
  margin-left: -8px;
  border: none;
  background: none;
  border-radius: var(--radius);
  cursor: pointer;
  transition: background var(--dur) var(--ease);
}
.top-brand:hover {
  background: var(--bg-hover);
}
.top-brand:active {
  background: var(--bg-active);
}

.top-logo {
  width: 28px;
  height: 28px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  border-radius: 8px;
  background: linear-gradient(135deg, var(--accent) 0%, var(--put) 100%);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.25);
}

.top-title {
  font-size: 14px;
  font-weight: 700;
  color: var(--text-1);
  letter-spacing: 0.01em;
}

.top-tag {
  font-size: 11px;
  color: var(--text-3);
  padding-left: 9px;
  border-left: 1px solid var(--border-strong);
  white-space: nowrap;
}

.top-right {
  margin-left: auto;
  display: flex;
  align-items: center;
}

/* ---------- 主体：导航 + 内容 ---------- */
.dash-body {
  flex: 1;
  min-height: 0;
  display: flex;
}

.dash-main {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 24px 28px 32px;
  display: flex;
  flex-direction: column;
  gap: 18px;
}

/* ---------- 摘要卡片（三列固定网格，统一高度与边距） ---------- */
.summary-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
  margin-bottom: 8px;
}
@media (max-width: 960px) {
  .summary-grid {
    grid-template-columns: 1fr;
  }
}

.stat-card {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 18px 22px;
  border-radius: var(--radius-lg);
  /* 半透明深底 + 1px 微光边框：比纯 var(--border) 更有层次 */
  border: 1px solid rgba(255, 255, 255, 0.06);
  background:
    linear-gradient(180deg, rgba(255, 255, 255, 0.025), rgba(255, 255, 255, 0) 55%),
    var(--bg-panel);
  box-shadow: var(--shadow);
  transition:
    border-color var(--dur) var(--ease),
    transform var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}
.stat-card:hover {
  border-color: rgba(255, 255, 255, 0.12);
  transform: translateY(-1px);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
}
html[data-theme='light'] .stat-card {
  border-color: var(--border);
}

/* 图标底座：圆角方块 + 分色微光（indigo / violet / amber 三档） */
.stat-icon {
  width: 40px;
  height: 40px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 12px;
  background: var(--accent-tint);
  color: var(--accent);
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
}
.stat-icon.tint-indigo {
  background: rgba(99, 102, 241, 0.13);
  color: #818cf8;
  box-shadow: inset 0 0 0 1px rgba(129, 140, 248, 0.22);
}
.stat-icon.tint-violet {
  background: rgba(167, 139, 250, 0.12);
  color: #a78bfa;
  box-shadow: inset 0 0 0 1px rgba(167, 139, 250, 0.2);
}
.stat-icon.tint-amber {
  background: rgba(245, 158, 11, 0.11);
  color: #fbbf24;
  box-shadow: inset 0 0 0 1px rgba(251, 191, 36, 0.18);
}

.stat-body {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}

/* ---- Hero 欢迎语 + 渐变大数字 ---- */
.hero-greet {
  min-width: 0;
}

.hero-title {
  margin: 0;
  font-size: 15px;
  font-weight: 700;
  color: var(--text-1);
  letter-spacing: 0.01em;
}

.hero-sub {
  margin: 3px 0 0;
  font-size: 11.5px;
  color: var(--text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.hero-stats {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-top: 12px;
}

.hero-stat {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
}

.hero-divider {
  width: 1px;
  align-self: stretch;
  background: var(--border);
}

/* 渐变数字：白 → 灰的纵向渐变文字（暗色主题下的「高级感」核心） */
.hero-value {
  font-family: var(--font-mono);
  font-size: 30px;
  font-weight: 800;
  line-height: 1.1;
  letter-spacing: -0.01em;
  font-variant-numeric: tabular-nums;
  background: linear-gradient(180deg, #ffffff 30%, rgba(255, 255, 255, 0.4));
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
html[data-theme='light'] .hero-value {
  background: linear-gradient(180deg, var(--text-1) 30%, var(--text-3));
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}

.hero-label {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11.5px;
  color: var(--text-2);
}

.stat-label {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 12px;
  color: var(--text-2);
}

.stat-sub {
  font-size: 11.5px;
  color: var(--text-3);
}

/* ---- 最近活动：时间线 + Method Badge ---- */
.timeline {
  display: flex;
  flex-direction: column;
  margin-top: 6px;
}

.tl-item {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 5px 8px;
  border: none;
  background: none;
  border-radius: var(--radius);
  cursor: pointer;
  font-family: inherit;
  text-align: left;
  transition: background var(--dur) var(--ease);
}
.tl-item + .tl-item {
  border-top: 1px dashed var(--border);
  border-top-left-radius: 0;
  border-top-right-radius: 0;
}
.tl-item:hover {
  background: rgba(255, 255, 255, 0.04);
}
html[data-theme='light'] .tl-item:hover {
  background: rgba(0, 0, 0, 0.04);
}

/* Method Badge：与 TabBar 的 method-tag 同配色体系 */
.tl-badge {
  flex-shrink: 0;
  width: 42px;
  text-align: center;
  font-family: var(--font-mono);
  font-size: 9.5px;
  font-weight: 700;
  line-height: 1;
  padding: 4px 0;
  border-radius: 6px;
  letter-spacing: 0.03em;
  color: var(--rf-text-muted);
  background: var(--bg-hover);
}
.tl-badge.get {
  color: var(--rf-success);
  background: var(--success-tint);
}
.tl-badge.post {
  color: var(--rf-warning);
  background: var(--warning-tint);
}
.tl-badge.put {
  color: var(--rf-info);
  background: var(--info-tint);
}
.tl-badge.delete {
  color: var(--rf-danger);
  background: var(--danger-tint);
}
.tl-badge.patch {
  color: var(--patch);
  background: var(--accent-tint);
}
.tl-badge.gql {
  color: #f472b6;
  background: rgba(236, 72, 153, 0.13);
}
.tl-badge.new {
  color: var(--text-3);
  background: var(--bg-hover);
}

.tl-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.tl-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.tl-name {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tl-time {
  font-size: 11px;
  color: var(--text-3);
  flex-shrink: 0;
}

.tl-path {
  font-family: var(--font-mono);
  font-size: 10.5px;
  color: var(--text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.quick-row {
  display: flex;
  gap: 8px;
  margin-top: 8px;
}

.quick-btn {
  flex: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 30px;
  padding: 0 10px;
  border-radius: var(--radius);
  font-size: 12px;
  font-weight: 600;
  font-family: inherit;
  white-space: nowrap;
  cursor: pointer;
  transition:
    background var(--dur) var(--ease),
    border-color var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease),
    transform var(--dur) var(--ease);
}
.quick-btn:active {
  transform: translateY(1px);
}

/* 主 CTA：violet 渐变 + 霓虹光晕（页面唯一的高饱和焦点） */
.quick-btn.primary {
  border: 1px solid rgba(139, 92, 246, 0.55);
  background: linear-gradient(135deg, rgba(124, 105, 245, 0.95), rgba(99, 102, 241, 0.95));
  color: #fff;
  box-shadow:
    0 4px 18px rgba(124, 105, 245, 0.3),
    inset 0 1px 0 rgba(255, 255, 255, 0.22);
}
.quick-btn.primary:hover {
  border-color: rgba(139, 92, 246, 0.85);
  background: linear-gradient(135deg, var(--accent-hover), rgba(109, 118, 245, 0.98));
  box-shadow:
    0 6px 26px rgba(124, 105, 245, 0.48),
    inset 0 1px 0 rgba(255, 255, 255, 0.28);
}

/* 次要动作（导入）：中性描边样式 */
.quick-btn.ghost {
  border: 1px solid var(--border-strong);
  background: rgba(255, 255, 255, 0.02);
  color: var(--text-2);
}
.quick-btn.ghost:hover {
  background: var(--bg-hover);
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  color: var(--accent);
}

/* ---------- 底部 Dropzone：拖入导入 + 点击创建 ---------- */
.add-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 14px 18px;
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius-lg);
  background: transparent;
  transition:
    border-color var(--dur) var(--ease),
    background var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease),
    transform var(--dur) var(--ease);
}
.add-card:hover {
  border-color: rgba(168, 85, 247, 0.4);
  background: rgba(124, 58, 237, 0.05);
}
/* 拖拽悬停：实线主色框 + 光晕 + 轻微上浮，明确「可以放了」 */
.add-card.drop-active {
  border-style: solid;
  border-color: var(--accent);
  background: var(--accent-tint);
  box-shadow:
    0 0 0 4px rgba(124, 105, 245, 0.12),
    0 8px 30px rgba(124, 105, 245, 0.18);
  transform: translateY(-1px);
}

.add-icon {
  display: inline-flex;
  color: var(--text-3);
  transition: color var(--dur) var(--ease);
}
.add-card:hover .add-icon {
  color: #a78bfa;
}

.add-text {
  margin: 0;
  flex: 1;
  min-width: 0;
  font-size: 12.5px;
  color: var(--text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: color var(--dur) var(--ease);
}
.add-card:hover .add-text {
  color: #c4b5fd;
}

.add-actions {
  display: flex;
  gap: 10px;
  flex-shrink: 0;
}

.add-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius);
  background: var(--bg-panel);
  color: var(--text-1);
  font-size: 12px;
  font-family: inherit;
  cursor: pointer;
  transition:
    border-color var(--dur) var(--ease),
    background var(--dur) var(--ease),
    color var(--dur) var(--ease);
}
.add-btn:hover {
  border-color: var(--accent);
  background: var(--accent-tint);
  color: #c4b5fd;
}

/* ---------- 工具栏：过滤 + 视图切换 + 排序 + 新建 ---------- */
.toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
}

.toolbar-filter {
  /* w-64：固定宽度，右侧留给主按钮 */
  width: 256px;
  display: flex;
  align-items: center;
  gap: 8px;
  height: 34px;
  padding: 0 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  background: var(--bg-card);
  color: var(--text-3);
  transition:
    border-color var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}
.toolbar-filter:focus-within {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-tint);
}

.toolbar-filter-input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: none;
  color: var(--text-1);
  font-family: inherit;
  font-size: 12.5px;
}
.toolbar-filter-input::placeholder {
  color: var(--text-3);
}

.btn-new {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  height: 34px;
  padding: 0 16px;
  border: none;
  border-radius: var(--radius-lg);
  background: var(--accent);
  color: #fff;
  font-size: 13px;
  font-weight: 600;
  font-family: inherit;
  cursor: pointer;
  box-shadow: 0 4px 14px var(--accent-tint);
  transition:
    background var(--dur) var(--ease),
    transform var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}
.btn-new:hover {
  background: var(--accent-hover);
  box-shadow: 0 6px 18px var(--accent-tint);
}
.btn-new:active {
  transform: translateY(1px);
}

/* ---------- 项目卡片网格（卡片自身样式见 ProjectCard） ---------- */
/* auto-fill + minmax(300px,1fr)：按容器宽度自适应列数；
   轨道下限用固定 300px 而非 auto，避免卡片内 nowrap 长描述把轨道
   撑出容器（曾导致横向滚动条、卡片被切出屏幕）。 */
.card-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 16px;
}

/* ---------- 拖拽排序 ---------- */
.card-grid :deep(.proj-card) {
  cursor: grab;
}
.card-grid :deep(.proj-card:active) {
  cursor: grabbing;
}

/* 拖拽中元素禁用自身 transform 过渡：SortableJS 独占移动控制，
   否则卡片 hover 缓动与拖拽动画互相干扰（拖动生硬、掉帧）。 */
:global(.sortable-ghost),
:global(.sortable-drag),
:global(.sortable-chosen) {
  transition: none !important;
  will-change: transform;
}

/* swap：交换目标卡片高亮反馈 */
:global(.sortable-swap) {
  border: 1px solid color-mix(in srgb, var(--accent) 50%, transparent) !important;
  background: color-mix(in srgb, var(--accent) 8%, transparent) !important;
  transition: none !important;
}

/* ghost：原位置镂空占位（ghost 即原卡片元素，内部全部隐藏 + 极淡底色） */
:global(.sortable-ghost) {
  background-color: rgba(168, 85, 247, 0.05) !important;
  border: 2px dashed rgba(168, 85, 247, 0.4) !important;
  border-radius: 0.75rem !important;
  box-shadow: none !important;
  opacity: 1;
  cursor: grabbing;
}
:global(.sortable-ghost *) {
  opacity: 0 !important;
  visibility: hidden !important;
}

/* chosen：mousedown 即给原元素反馈（拖起瞬间轻微放大） */
:global(.sortable-chosen) {
  transform: scale(1.03);
  cursor: grabbing;
}

/* drag：被拖主体（fallbackClass 克隆挂 body，悬浮感最强） */
:global(.sortable-drag) {
  z-index: 999;
  border-radius: var(--radius-lg) !important;
  background: rgba(38, 38, 38, 0.95) !important;
  -webkit-backdrop-filter: blur(8px);
  backdrop-filter: blur(8px);
  border: 1px solid #a855f7 !important;
  box-shadow:
    0 20px 50px rgba(0, 0, 0, 0.8),
    0 0 40px rgba(59, 7, 100, 0.4) !important;
  transform: scale(1.03) rotate(1.5deg);
  cursor: grabbing;
}

/* flip：其余卡片平滑挤开（animation 200ms 已生效，此为兜底） */
.card-grid :deep(.flip-list-move) {
  transition: transform 0.2s cubic-bezier(0.2, 0, 0, 1);
}

/* ---------- 空状态 ---------- */
.dash-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 48px 0;
  text-align: center;
}

.empty-icon {
  color: var(--text-3);
  margin-bottom: 6px;
}

.empty-title {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  color: var(--text-1);
}

.empty-hint {
  margin: 0 0 10px;
  font-size: 12.5px;
  color: var(--text-3);
  max-width: 380px;
}
</style>
