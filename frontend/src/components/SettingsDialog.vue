<script setup lang="ts">
/**
 * SettingsDialog：设置弹框（Linear / Raycast 风格暗黑双栏设置面板）。
 *
 * 左栏：轻量 Menu List 导航（扁平行高 + 左侧紫色指示条）；
 * 右栏：卡片化设置组，Tab 切换淡入；简单项改动即自动保存。
 */
import { computed, onMounted, ref, watch } from 'vue'
import { join } from '@tauri-apps/api/path'
import { open } from '@tauri-apps/plugin-dialog'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import { useFoxApi } from '../composables/useFoxApi'
import { useToast } from '../composables/useToast'
import { useThemeStore, type ThemeMode } from '../stores/theme'
import EnvironmentManager from './EnvironmentManager.vue'
import Modal from './ui/Modal.vue'
import Icon, { type IconName } from './ui/Icon.vue'
import CustomNumberInput from './ui/CustomNumberInput.vue'
import { envBaseUrl } from '../utils/environment'
import type { Environment, Project, ProjectStat, SeqCounter } from '../types/foxApi'

const emit = defineEmits<{ close: [] }>()

const api = useFoxApi()
const toast = useToast()
const theme = useThemeStore()

const THEME_OPTIONS: { value: ThemeMode; label: string; icon: string }[] = [
  { value: 'system', label: '跟随系统', icon: '💻' },
  { value: 'dark', label: '深色', icon: '🌙' },
  { value: 'light', label: '浅色', icon: '☀️' },
]

// ---------- 分类导航 ----------
type TabId = 'general' | 'network' | 'sequences' | 'data' | 'environments'
interface TabDef {
  id: TabId
  label: string
  icon: IconName
}
const tabs: TabDef[] = [
  { id: 'general', label: '通用设置', icon: 'settings' },
  { id: 'network', label: '网络与代理', icon: 'globe' },
  { id: 'sequences', label: '自增序列', icon: 'list' },
  { id: 'data', label: '数据与备份', icon: 'folder' },
  { id: 'environments', label: '环境管理', icon: 'beaker' },
]
const activeTab = ref<TabId>('general')

const showManager = ref(false)
/** 「编辑」某环境时，传给 EnvironmentManager 让它初始聚焦该环境。 */
const managerEnvId = ref<string | null>(null)

const project = ref<Project | null>(null)
const projectStat = ref<ProjectStat | null>(null)
const busy = ref(false)
const fileInput = ref<HTMLInputElement | null>(null)

// ---------- 环境概览 ----------
const environments = ref<Environment[]>([])
const activeEnvId = ref<string | null>(null)
const envLoading = ref(false)

async function loadEnvironments(): Promise<void> {
  envLoading.value = true
  try {
    const [envs, active] = await Promise.all([
      api.listEnvironments(),
      api.getActiveEnvironment(),
    ])
    environments.value = envs
    activeEnvId.value = active?.id ?? null
  } catch {
    environments.value = []
    activeEnvId.value = null
  } finally {
    envLoading.value = false
  }
}

/** 打开环境管理弹窗；envId 为空则聚焦当前激活（或第一个）环境。 */
function openEnvironmentManager(envId: string | null = null): void {
  managerEnvId.value = envId
  showManager.value = true
}

/** 环境概览辅助：本项目视角的默认模块基址。 */
function envBase(env: Environment): string {
  return envBaseUrl(env, project.value?.id)
}

/** 环境概览辅助：启用中的变量数量。 */
function envVarCount(env: Environment): number {
  return env.variables.filter((v) => v.enabled).length
}

// 管理弹窗关闭后刷新概览（可能新增/删除/改激活环境）
watch(showManager, (open) => {
  if (!open) void loadEnvironments()
})

onMounted(async () => {
  try {
    project.value = (await api.getActiveProject()) ?? null
  } catch {
    project.value = null
  }
  if (project.value) {
    try {
      const stats = await api.listProjectStats()
      projectStat.value = stats.find((s) => s.project_id === project.value?.id) ?? null
    } catch {
      projectStat.value = null
    }
  }
  try {
    proxyUrl.value = (await api.getHttpProxy()) ?? ''
    proxyEnabled.value = !!proxyUrl.value
  } catch {
    proxyUrl.value = ''
    proxyEnabled.value = false
  }
  try {
    const ms = await api.getHttpTimeoutMs()
    timeoutSec.value = ms != null ? Math.round(ms / 1000) : DEFAULT_TIMEOUT_SEC
  } catch {
    timeoutSec.value = DEFAULT_TIMEOUT_SEC
  }
  await loadEnvironments()
  await loadCounters()
})

// ---------- 通用设置：请求超时（自动保存） ----------
const DEFAULT_TIMEOUT_SEC = 300
const timeoutSec = ref(DEFAULT_TIMEOUT_SEC)

async function saveTimeout(sec: number): Promise<void> {
  const v = Math.round(Number(sec))
  if (!Number.isFinite(v) || v < 1 || v > 3600) {
    toast.error('超时需在 1 ~ 3600 秒之间')
    return
  }
  try {
    await api.setHttpTimeoutMs(v * 1000)
    timeoutSec.value = v
    toast.success(`已保存：请求超时 ${v} 秒`)
  } catch (err) {
    toast.error('保存失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

// ---------- 网络与代理 ----------
const proxyUrl = ref('')
const proxyEnabled = ref(false)
const proxyBusy = ref(false)
const proxyTesting = ref(false)
const proxyTest = ref<{ ok: boolean; message: string } | null>(null)

async function applyProxy(url: string | null): Promise<boolean> {
  proxyBusy.value = true
  try {
    await api.setHttpProxy(url)
    return true
  } catch (err) {
    toast.error('代理保存失败', { message: err instanceof Error ? err.message : String(err) })
    return false
  } finally {
    proxyBusy.value = false
  }
}

/** 启用开关：关闭 → 直连；开启 → 沿用已有地址（为空则仅展开输入框）。 */
async function toggleProxy(): Promise<void> {
  if (proxyEnabled.value) {
    proxyEnabled.value = false
    if (await applyProxy(null)) toast.success('已切换为直连')
    proxyTest.value = null
  } else {
    proxyEnabled.value = true
    if (proxyUrl.value.trim()) {
      if (await applyProxy(proxyUrl.value.trim())) toast.success('代理已启用')
    }
  }
}

/** URL 失焦自动保存。 */
async function saveProxyUrl(): Promise<void> {
  const u = proxyUrl.value.trim()
  if (!proxyEnabled.value) return
  if (u && !/^(https?|socks5?):\/\//i.test(u)) {
    toast.error('代理地址需以 http:// 或 socks5:// 开头')
    return
  }
  if (await applyProxy(u || null)) toast.success(u ? '代理已保存' : '已切换为直连')
}

/** 测试连通性：先落地当前输入，再经共享客户端（含代理）请求目标。 */
async function testProxy(): Promise<void> {
  if (proxyEnabled.value && proxyUrl.value.trim()) {
    await applyProxy(proxyUrl.value.trim())
  }
  proxyTesting.value = true
  proxyTest.value = null
  try {
    const r = await api.testHttpProxy()
    proxyTest.value = { ok: r.ok, message: r.message }
    if (r.ok) {
      toast.success('代理连通正常')
    } else {
      toast.error(r.message)
    }
  } catch (err) {
    proxyTest.value = { ok: false, message: err instanceof Error ? err.message : String(err) }
    toast.error('测试失败', { message: proxyTest.value.message })
  } finally {
    proxyTesting.value = false
  }
}

// ---------- 自增序列 ----------
const counters = ref<SeqCounter[]>([])
const newSeqKey = ref('')
const newSeqValue = ref<number>(1)

async function loadCounters(): Promise<void> {
  try {
    counters.value = await api.listSeqCounters()
  } catch {
    counters.value = []
  }
}

async function setCounter(key: string, value: number, message: string): Promise<void> {
  try {
    await api.setSeqCounter(key, Math.max(1, Math.round(value)))
    toast.success(message)
  } catch (err) {
    toast.error('保存失败', { message: err instanceof Error ? err.message : String(err) })
  } finally {
    await loadCounters()
  }
}

function saveSeq(c: SeqCounter): void {
  void setCounter(c.key, c.value, c.key ? `序列「${c.key}」已保存` : '全局序列已保存')
}

function resetSeq(c: SeqCounter): void {
  void setCounter(c.key, 1, c.key ? `序列「${c.key}」已重置为 1` : '全局序列已重置为 1')
}

async function deleteSeq(c: SeqCounter): Promise<void> {
  try {
    await api.deleteSeqCounter(c.key)
    toast.success(c.key ? `序列「${c.key}」已删除` : '全局序列已重置')
  } catch (err) {
    toast.error('删除失败', { message: err instanceof Error ? err.message : String(err) })
  } finally {
    await loadCounters()
  }
}

async function addSeq(): Promise<void> {
  const key = newSeqKey.value.trim()
  const value = Math.round(Number(newSeqValue.value))
  if (!Number.isFinite(value) || value < 1) {
    toast.error('起始值需 ≥ 1')
    return
  }
  try {
    await api.setSeqCounter(key, value)
    toast.success(key ? `序列「${key}」已添加，从 ${value} 开始` : '全局序列已从该值开始')
    newSeqKey.value = ''
    newSeqValue.value = 1
  } catch (err) {
    toast.error('添加失败', { message: err instanceof Error ? err.message : String(err) })
  } finally {
    await loadCounters()
  }
}

// ---------- 数据与备份 ----------
async function exportBackup(): Promise<void> {
  if (!project.value) return
  busy.value = true
  try {
    const text = await api.backupExport(project.value.id)
    const stamp = new Date().toISOString().slice(0, 10)
    const filename = `${project.value.name}-备份-${stamp}.json`

    // Tauri 环境：目录选择框选目标文件夹，再拼接默认文件名经 save_text_file 落盘。
    if ('__TAURI_INTERNALS__' in window) {
      const dir = await open({
        directory: true,
        title: '选择备份保存目录',
      })
      if (!dir) return // 用户取消
      const path = await join(dir, filename)
      await api.writeTextFile(path, text)
      toast.success('✓ 备份已导出', {
        message: path.split('/').pop() || path,
        action: {
          label: '打开文件位置',
          run: () => {
            void revealItemInDir(path).catch(() => toast.error('无法定位文件'))
          },
        },
      })
      return
    }

    // 浏览器预览兜底：Blob 下载
    const blob = new Blob([text], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = filename
    a.click()
    URL.revokeObjectURL(url)
    toast.success('备份已导出')
  } catch (err) {
    toast.error('导出失败', { message: err instanceof Error ? err.message : String(err) })
  } finally {
    busy.value = false
  }
}

async function onImportFile(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  busy.value = true
  try {
    const text = await file.text()
    const summary = await api.backupRestore(text)
    toast.success(
      `已恢复为「${summary.name}」：接口 ${summary.endpoints} 个、环境 ${summary.environments} 个`,
    )
    emit('close')
  } catch (err) {
    toast.error('导入失败', { message: err instanceof Error ? err.message : String(err) })
  } finally {
    busy.value = false
  }
}

// ---------- 通用派生 ----------
const sequencesCount = computed(() => counters.value.length)
const projectSummary = computed(() => {
  if (!project.value) return '未选择项目，请先进入任一项目工作区'
  const eps = projectStat.value?.endpoint_count ?? '—'
  return `${project.value.name} · ${eps} 个接口`
})
</script>

<template>
  <Modal :open="true" title="设置" width="880px" dialog-class="sd-dialog" @close="emit('close')">
    <div class="flex h-[min(520px,70vh)]">
      <!-- 左：轻量 Menu List 导航（Linear / VS Code 风格） -->
      <aside class="w-48 shrink-0 overflow-y-auto border-r border-zinc-200/80 px-2 py-3 dark:border-white/[0.06]">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          type="button"
          class="relative flex h-9 w-full items-center gap-2.5 rounded-md px-3 py-1.5 text-left text-[13px] transition-colors duration-150"
          :class="
            activeTab === tab.id
              ? 'bg-purple-500/10 font-semibold text-purple-600 dark:bg-purple-500/10 dark:text-purple-300'
              : 'border-none bg-transparent text-zinc-600 hover:bg-zinc-100/80 hover:text-zinc-900 dark:text-zinc-400 dark:hover:bg-white/[0.05] dark:hover:text-zinc-200'
          "
          @click="activeTab = tab.id"
        >
          <span
            v-if="activeTab === tab.id"
            class="absolute left-0 top-1/2 h-4 w-0.5 -translate-y-1/2 rounded-r bg-purple-600 dark:bg-purple-500"
          />
          <Icon
            :name="tab.icon"
            :size="15"
            :class="activeTab === tab.id ? 'text-purple-600 dark:text-purple-300' : 'text-zinc-500'"
          />
          <span class="flex-1">{{ tab.label }}</span>
          <span
            v-if="tab.id === 'sequences' && sequencesCount"
            class="rounded-full bg-zinc-200/70 px-1.5 py-px text-[11px] leading-4 text-zinc-600 dark:bg-white/[0.08] dark:text-zinc-400"
          >
            {{ sequencesCount }}
          </span>
        </button>
      </aside>

      <!-- 右：内容区（Tab 切换淡入） -->
      <div class="flex-1 min-w-0 overflow-y-auto p-6">
        <Transition name="pane" mode="out-in">
          <div :key="activeTab" class="space-y-4">
            <!-- 通用设置 -->
            <section v-if="activeTab === 'general'">
              <header>
                <h2 class="text-base font-medium text-zinc-900 dark:text-zinc-100">通用设置</h2>
                <p class="mt-1 mb-5 text-xs text-zinc-600 dark:text-zinc-500">应用级请求与外观偏好。</p>
              </header>

              <div class="rounded-xl border border-zinc-200/70 bg-zinc-50/80 p-5 dark:border-white/[0.06] dark:bg-zinc-900/40">
                <div class="flex items-center justify-between gap-4">
                  <div class="max-w-md">
                    <div class="text-sm font-medium text-zinc-900 dark:text-zinc-100">请求超时</div>
                    <p class="mt-0.5 text-xs text-zinc-600 dark:text-zinc-400">
                      全局默认请求超时，应用于所有接口；改动即自动保存。
                    </p>
                  </div>
                  <div class="relative shrink-0">
                    <CustomNumberInput
                      :model-value="timeoutSec"
                      :min="1"
                      :max="3600"
                      :step="10"
                      size="md"
                      tone="inset"
                      class="w-24"
                      @change="saveTimeout"
                    />
                    <span
                      class="pointer-events-none absolute right-[28px] top-1/2 -translate-y-1/2 text-xs text-zinc-500"
                    >
                      秒
                    </span>
                  </div>
                </div>
              </div>

              <div class="rounded-xl border border-zinc-200/70 bg-zinc-50/80 p-5 dark:border-white/[0.06] dark:bg-zinc-900/40">
                <div class="flex items-center justify-between gap-4">
                  <div class="max-w-md">
                    <div class="text-sm font-medium text-zinc-900 dark:text-zinc-100">主题外观</div>
                    <p class="mt-0.5 text-xs text-zinc-600 dark:text-zinc-400">跟随系统或手动切换深色 / 浅色，即时生效。</p>
                  </div>
                  <div
                    class="flex shrink-0 items-center gap-1 rounded-lg border border-zinc-300/50 bg-zinc-200/60 p-1 dark:border-white/10 dark:bg-black/30"
                    role="radiogroup"
                    aria-label="主题外观"
                  >
                    <button
                      v-for="opt in THEME_OPTIONS"
                      :key="opt.value"
                      type="button"
                      role="radio"
                      :aria-checked="theme.mode === opt.value"
                      class="flex h-8 items-center gap-1.5 rounded-md px-3 text-xs transition-all duration-150"
                      :class="
                        theme.mode === opt.value
                          ? 'bg-purple-600 font-medium text-white shadow-sm'
                          : 'text-zinc-600 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-200'
                      "
                      @click="theme.setMode(opt.value)"
                    >
                      <span aria-hidden="true">{{ opt.icon }}</span>
                      {{ opt.label }}
                    </button>
                  </div>
                </div>
                <div class="mt-5 border-t border-zinc-200/70 dark:border-white/[0.06]">
                  <div class="flex items-center justify-between gap-4 pt-5 opacity-40">
                    <div class="max-w-md">
                      <div class="text-sm font-medium text-zinc-900 dark:text-zinc-100">快捷键</div>
                      <p class="mt-0.5 text-xs text-zinc-600 dark:text-zinc-400">发送请求、聚焦地址栏等全局快捷键。</p>
                    </div>
                    <span
                      class="rounded-full border border-zinc-200 bg-zinc-100 px-2 py-0.5 text-[11px] text-zinc-500 dark:border-white/[0.05] dark:bg-white/[0.03] dark:text-zinc-500"
                    >
                      即将推出
                    </span>
                  </div>
                </div>
              </div>
            </section>

            <!-- 网络与代理 -->
            <section v-if="activeTab === 'network'">
              <header>
                <h2 class="text-base font-medium text-zinc-900 dark:text-zinc-100">网络与代理</h2>
                <p class="mt-1 mb-5 text-xs text-zinc-600 dark:text-zinc-500">配置全局 HTTP / SOCKS5 代理，应用于所有请求。</p>
              </header>

              <div class="rounded-xl border border-zinc-200/70 bg-zinc-50/80 p-5 dark:border-white/[0.06] dark:bg-zinc-900/40">
                <div class="flex items-center justify-between gap-4">
                  <div class="max-w-md">
                    <div class="text-sm font-medium text-zinc-900 dark:text-zinc-100">启用代理</div>
                    <p class="mt-0.5 text-xs text-zinc-600 dark:text-zinc-400">
                      开启后所有请求经代理发出；关闭立即恢复直连，改动即保存。
                    </p>
                  </div>
                  <button
                    type="button"
                    role="switch"
                    :aria-checked="proxyEnabled"
                    class="relative h-[22px] w-[40px] shrink-0 rounded-full transition-colors duration-150"
                    :class="proxyEnabled ? 'bg-purple-500' : 'border border-zinc-300 bg-zinc-200 dark:border-white/10 dark:bg-white/10'"
                    @click="toggleProxy"
                  >
                    <span
                      class="absolute top-1/2 h-[16px] w-[16px] -translate-y-1/2 rounded-full bg-white shadow transition-all duration-150"
                      :class="proxyEnabled ? 'left-[21px]' : 'left-[2px]'"
                    />
                  </button>
                </div>

                <div v-if="proxyEnabled" class="mt-5 border-t border-zinc-200/70 dark:border-white/[0.06]">
                  <div class="flex items-center justify-between gap-4 pt-5">
                    <div class="max-w-md">
                      <div class="text-sm font-medium text-zinc-900 dark:text-zinc-100">代理地址</div>
                      <p class="mt-0.5 text-xs text-zinc-600 dark:text-zinc-400">
                        如 <code class="font-mono text-[11px]">http://127.0.0.1:7890</code> 或
                        <code class="font-mono text-[11px]">socks5://host:1080</code>；失焦自动保存。
                      </p>
                    </div>
                    <input
                      v-model="proxyUrl"
                      class="rf-input h-8 w-72 shrink-0 font-mono text-[12.5px]"
                      type="text"
                      placeholder="http://127.0.0.1:7890"
                      spellcheck="false"
                      @change="saveProxyUrl"
                    />
                  </div>

                  <div class="flex items-center justify-between gap-4 border-t border-zinc-200/70 pt-5 dark:border-white/[0.06]">
                    <div class="max-w-md">
                      <div class="text-sm font-medium text-zinc-900 dark:text-zinc-100">连通性测试</div>
                      <p class="mt-0.5 text-xs text-zinc-600 dark:text-zinc-400">
                        {{ proxyTest ? proxyTest.message : '经当前代理请求一次公开目标，验证可用性。' }}
                      </p>
                    </div>
                    <button
                      class="rf-btn shrink-0"
                      type="button"
                      :disabled="proxyBusy || proxyTesting"
                      @click="testProxy"
                    >
                      <Icon name="zap" :size="13" />
                      {{ proxyTesting ? '测试中…' : '测试连通性' }}
                    </button>
                  </div>
                </div>
              </div>
            </section>

            <!-- 自增序列 -->
            <section v-if="activeTab === 'sequences'">
              <header>
                <h2 class="text-base font-medium text-zinc-900 dark:text-zinc-100">自增序列与变量</h2>
                <p class="mt-1 mb-5 text-xs text-zinc-600 dark:text-zinc-500">
                  请求中写 <code class="font-mono text-[11px]">&#123;&#123;$seq:key&#125;&#125;</code> 自动递增；
                  值即「下一次输出」，持久化、重启不丢。
                </p>
              </header>

              <div class="rounded-xl border border-zinc-200/70 bg-zinc-50/80 p-5 dark:border-white/[0.06] dark:bg-zinc-900/40">
                <div class="text-sm font-medium text-zinc-900 dark:text-zinc-100">序列列表</div>

                <div v-if="counters.length" class="mt-3">
                  <div
                    class="mb-3 flex items-center gap-2 border-b border-zinc-200/70 pb-2 text-xs font-medium text-zinc-500 dark:border-white/[0.06] dark:text-zinc-400"
                  >
                    <div class="min-w-0 flex-1">序列 Key</div>
                    <div class="w-40 text-right">当前值 / 下一次输出</div>
                    <div class="w-24 text-right">操作</div>
                  </div>

                  <div
                    v-for="c in counters"
                    :key="c.key || '__global__'"
                    class="flex items-center gap-2 border-b border-zinc-200/70 py-2 last:border-b-0 dark:border-white/[0.06]"
                  >
                    <div class="flex min-w-0 flex-1 items-center gap-1.5 font-mono text-zinc-900 dark:text-zinc-100">
                      <span
                        class="shrink-0 rounded bg-purple-100 px-1.5 py-px text-[10px] font-medium text-purple-700 dark:bg-purple-500/15 dark:text-purple-400"
                      >
                        全局
                      </span>
                      <span v-if="!c.key" class="truncate">$seq</span>
                      <span v-else class="truncate">{{ c.key }}</span>
                    </div>
                    <div class="flex w-40 justify-end">
                      <input
                        v-model.number="c.value"
                        class="h-8 w-24 rounded-md border border-zinc-300 bg-white text-center font-mono text-xs text-zinc-800 tabular-nums outline-none transition-colors focus:border-purple-500 dark:border-white/10 dark:bg-black/30 dark:text-white"
                        type="number"
                        min="1"
                        spellcheck="false"
                        @change="saveSeq(c)"
                      />
                    </div>
                    <div class="flex w-24 items-center justify-end gap-1">
                      <button
                        class="rf-btn rf-btn-sm rf-btn-ghost"
                        type="button"
                        title="重置为 1"
                        @click="resetSeq(c)"
                      >
                        <Icon name="refresh" :size="12" />
                      </button>
                      <button
                        class="rf-btn rf-btn-sm rf-btn-ghost"
                        type="button"
                        title="删除序列"
                        @click="deleteSeq(c)"
                      >
                        <Icon name="trash" :size="12" />
                      </button>
                    </div>
                  </div>
                </div>

                <div
                  v-else
                  class="flex flex-col items-center justify-center rounded-lg py-8 text-center"
                >
                  <Icon name="list" :size="22" class="mb-2 text-zinc-400 dark:text-zinc-600" />
                  <div class="text-sm text-zinc-700 dark:text-zinc-300">暂无自定义序列</div>
                  <div class="mt-0.5 text-xs text-zinc-600 dark:text-zinc-500">下方输入 Key 名并设置起始值即可创建</div>
                </div>

                <div class="mt-4 border-t border-zinc-200/70 pt-4 dark:border-white/[0.06]">
                  <div class="flex items-center gap-2 rounded-lg border border-zinc-200/70 bg-zinc-100/80 p-2 dark:border-white/[0.06] dark:bg-black/20">
                    <input
                      v-model="newSeqKey"
                      class="h-8 min-w-0 flex-1 rounded-md border border-zinc-300 bg-white px-3 text-xs text-zinc-800 placeholder:text-zinc-400 outline-none transition-colors focus:border-purple-500 dark:border-white/10 dark:bg-black/30 dark:text-white dark:placeholder:text-zinc-600"
                      type="text"
                      placeholder="序列 key（留空 = 全局 $seq）"
                      spellcheck="false"
                      @keydown.enter="addSeq"
                    />
                    <input
                      v-model.number="newSeqValue"
                      class="h-8 w-20 shrink-0 rounded-md border border-zinc-300 bg-white text-center font-mono text-xs text-zinc-800 tabular-nums placeholder:text-zinc-400 outline-none transition-colors focus:border-purple-500 dark:border-white/10 dark:bg-black/30 dark:text-white dark:placeholder:text-zinc-600"
                      type="number"
                      min="1"
                      placeholder="起始值"
                      spellcheck="false"
                      @keydown.enter="addSeq"
                    />
                    <button
                      class="flex h-8 shrink-0 items-center gap-1 rounded-md bg-purple-600/80 px-3 text-xs font-medium text-white shadow-sm transition-all hover:bg-purple-600"
                      type="button"
                      @click="addSeq"
                    >
                      <Icon name="plus" :size="13" />
                      添加新序列
                    </button>
                  </div>
                  <p class="mt-2 text-right text-[11px] text-zinc-600 dark:text-zinc-500">改动失焦自动保存</p>
                </div>
              </div>
            </section>

            <!-- 数据与备份 -->
            <section v-if="activeTab === 'data'">
              <header>
                <h2 class="text-base font-medium text-zinc-900 dark:text-zinc-100">数据与备份</h2>
                <p class="mt-1 mb-5 text-xs text-zinc-600 dark:text-zinc-500">导出当前项目备份，或从备份文件恢复。</p>
              </header>

              <div class="rounded-xl border border-zinc-200/70 bg-zinc-50/80 p-5 dark:border-white/[0.06] dark:bg-zinc-900/40">
                <div class="flex items-center gap-3">
                  <div
                    class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-purple-100 text-purple-700 dark:bg-purple-500/15 dark:text-purple-400"
                  >
                    <Icon name="package" :size="18" />
                  </div>
                  <div class="min-w-0">
                    <div class="truncate text-sm font-medium text-zinc-900 dark:text-zinc-100">
                      {{ project ? project.name : '未选择项目' }}
                    </div>
                    <p class="mt-0.5 truncate text-xs text-zinc-600 dark:text-zinc-400">{{ projectSummary }}</p>
                  </div>
                </div>

                <div class="mt-4 flex items-center gap-2 border-t border-zinc-200/70 pt-4 dark:border-white/[0.06]">
                  <button
                    class="rf-btn"
                    type="button"
                    :disabled="!project || busy"
                    @click="exportBackup"
                  >
                    <Icon name="download" :size="13" />
                    {{ busy ? '处理中…' : '导出备份' }}
                  </button>
                  <button class="rf-btn" type="button" :disabled="busy" @click="fileInput?.click()">
                    <Icon name="upload" :size="13" />
                    导入恢复
                  </button>
                  <input
                    ref="fileInput"
                    type="file"
                    accept=".json,application/json"
                    class="hidden"
                    @change="onImportFile"
                  />
                </div>
              </div>
            </section>

            <!-- 环境管理 -->
            <section v-if="activeTab === 'environments'">
              <header>
                <h2 class="text-base font-medium text-zinc-900 dark:text-zinc-100">环境管理</h2>
                <p class="mt-1 mb-5 text-xs text-zinc-600 dark:text-zinc-500">查看当前项目下的环境配置及生效变量。</p>
              </header>

              <div class="space-y-2">
                <div
                  v-for="env in environments"
                  :key="env.id"
                  class="flex items-center justify-between gap-3 rounded-lg border border-zinc-200/70 bg-zinc-50/80 p-3.5 dark:border-white/[0.06] dark:bg-zinc-900/40"
                >
                  <div class="flex min-w-0 items-center gap-2.5">
                    <span
                      class="sd-dot"
                      :class="env.id === activeEnvId ? 'sd-dot-active' : ''"
                      aria-hidden="true"
                    ></span>
                    <div class="min-w-0">
                      <div class="flex items-center gap-1.5">
                        <span class="truncate text-sm font-medium text-zinc-900 dark:text-zinc-100">{{ env.name }}</span>
                        <span
                          v-if="env.id === activeEnvId"
                          class="rounded-full bg-emerald-100 px-1.5 py-px text-[10px] font-medium text-emerald-700 dark:bg-emerald-500/10 dark:text-emerald-400"
                        >
                          当前
                        </span>
                      </div>
                      <div class="truncate font-mono text-[11px] text-zinc-600 dark:text-zinc-500">
                        {{ envBase(env) || '未配置 Base URL' }}
                      </div>
                    </div>
                  </div>
                  <div class="flex shrink-0 items-center gap-2">
                    <span
                      class="rounded-full bg-zinc-200/70 px-2 py-0.5 text-[11px] text-zinc-600 dark:bg-white/[0.04] dark:text-zinc-400"
                    >
                      {{ envVarCount(env) }} 个变量
                    </span>
                    <button
                      type="button"
                      class="rounded-md px-2 py-1 text-[11px] text-zinc-500 transition-colors duration-150 hover:bg-zinc-100 hover:text-zinc-900 dark:text-zinc-400 dark:hover:bg-white/[0.06] dark:hover:text-zinc-200"
                      @click="openEnvironmentManager(env.id)"
                    >
                      编辑
                    </button>
                  </div>
                </div>

                <div
                  v-if="!environments.length && !envLoading"
                  class="rounded-lg border border-zinc-200/70 bg-zinc-50/80 p-6 text-center text-xs text-zinc-600 dark:border-white/[0.06] dark:bg-zinc-900/40 dark:text-zinc-500"
                >
                  暂无环境，点击下方「打开高级环境管理」创建。
                </div>
              </div>

              <div class="mt-4 border-t border-zinc-200/70 pt-4 dark:border-white/[0.06]">
                <button
                  class="rf-btn w-full"
                  type="button"
                  @click="openEnvironmentManager()"
                >
                  <Icon name="settings" :size="13" />
                  打开高级环境管理
                </button>
              </div>
            </section>
          </div>
        </Transition>
      </div>
    </div>

    <EnvironmentManager v-model:open="showManager" :initial-env-id="managerEnvId" />
  </Modal>
</template>

<style scoped>
.pane-enter-active,
.pane-leave-active {
  transition:
    opacity 160ms var(--ease),
    transform 160ms var(--ease);
}
.pane-enter-from {
  opacity: 0;
  transform: translateY(4px);
}
.pane-leave-to {
  opacity: 0;
  transform: translateY(-2px);
}

/* 环境概览状态点：默认灰，激活绿 + 柔光 */
.sd-dot {
  width: 8px;
  height: 8px;
  border-radius: 9999px;
  background: var(--text-3);
  flex-shrink: 0;
}
.sd-dot-active {
  background: var(--success);
  box-shadow: 0 0 0 3px var(--success-tint);
}
</style>

<style>
/* 设置弹窗容器：覆写 Modal 的 CSS 变量。
   深色 = 深邃高级暗色底 + 极细白描边 + 柔和深影；浅色 = macOS 原生白色面板质感。 */
.sd-dialog {
  --bg-elevated: #121215;
  --border-strong: rgba(255, 255, 255, 0.1);
  --radius-lg: 16px;
  --shadow-lg: 0 25px 60px -12px rgba(0, 0, 0, 0.8);
}
html[data-theme='light'] .sd-dialog {
  --bg-elevated: #ffffff;
  --border-strong: rgba(24, 24, 27, 0.12);
  --shadow-lg: 0 25px 60px -12px rgba(0, 0, 0, 0.18);
}
</style>
