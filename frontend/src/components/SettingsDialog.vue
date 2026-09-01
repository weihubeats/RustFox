<script setup lang="ts">
/**
 * SettingsDialog：设置弹框（Linear / Raycast 风格暗黑双栏设置面板）。
 *
 * 左栏：轻量 Menu List 导航（扁平行高 + 左侧紫色指示条）；
 * 右栏：卡片化设置组，Tab 切换淡入；简单项改动即自动保存。
 */
import { computed, onMounted, ref } from 'vue'
import { join } from '@tauri-apps/api/path'
import { open } from '@tauri-apps/plugin-dialog'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import { useFoxApi } from '../composables/useFoxApi'
import { useToast } from '../composables/useToast'
import EnvironmentManager from './EnvironmentManager.vue'
import Modal from './ui/Modal.vue'
import Icon, { type IconName } from './ui/Icon.vue'
import CustomNumberInput from './ui/CustomNumberInput.vue'
import type { Project, ProjectStat, SeqCounter } from '../types/foxApi'

const emit = defineEmits<{ close: [] }>()

const api = useFoxApi()
const toast = useToast()

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

const project = ref<Project | null>(null)
const projectStat = ref<ProjectStat | null>(null)
const busy = ref(false)
const fileInput = ref<HTMLInputElement | null>(null)

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
  <Modal :open="true" title="设置" width="880px" @close="emit('close')">
    <div class="flex h-[min(520px,70vh)]">
      <!-- 左：轻量 Menu List 导航 -->
      <aside class="w-52 shrink-0 overflow-y-auto border-r border-white/5 px-2 py-4">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          type="button"
          class="relative flex h-9 w-full items-center gap-2.5 rounded-lg px-3 text-left text-[13px] transition-colors duration-150"
          :class="
            activeTab === tab.id
              ? 'bg-white/10 font-medium text-white'
              : 'text-zinc-400 hover:bg-white/5 hover:text-zinc-200'
          "
          @click="activeTab = tab.id"
        >
          <span
            v-if="activeTab === tab.id"
            class="absolute -left-0.5 top-1/2 h-4 w-[2px] -translate-y-1/2 rounded-full bg-purple-500"
          />
          <Icon
            :name="tab.icon"
            :size="15"
            :class="activeTab === tab.id ? 'text-purple-400' : 'text-zinc-500'"
          />
          <span class="flex-1">{{ tab.label }}</span>
          <span
            v-if="tab.id === 'sequences' && sequencesCount"
            class="rounded-full bg-white/10 px-1.5 py-px text-[11px] leading-4 text-zinc-400"
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
              <header class="mb-6">
                <h2 class="text-lg font-semibold leading-tight text-zinc-100">通用设置</h2>
                <p class="mt-1 text-xs text-zinc-500">应用级请求与外观偏好。</p>
              </header>

              <div class="rounded-xl border border-white/5 bg-zinc-900/50 p-1">
                <div class="flex items-center justify-between px-4 py-4">
                  <div class="max-w-md">
                    <div class="text-sm font-medium text-zinc-100">请求超时</div>
                    <p class="mt-0.5 text-xs text-zinc-400">
                      全局默认请求超时，应用于所有接口；改动即自动保存。
                    </p>
                  </div>
                  <div class="relative">
                    <CustomNumberInput
                      :model-value="timeoutSec"
                      :min="1"
                      :max="3600"
                      :step="10"
                      size="md"
                      class="w-28"
                      @change="saveTimeout"
                    />
                    <span
                      class="pointer-events-none absolute right-[30px] top-1/2 -translate-y-1/2 text-[11px] text-zinc-500"
                    >
                      秒
                    </span>
                  </div>
                </div>
              </div>

              <div class="rounded-xl border border-white/5 bg-zinc-900/50 p-1">
                <div class="flex items-center justify-between px-4 py-4 opacity-40">
                  <div class="max-w-md">
                    <div class="text-sm font-medium text-zinc-100">主题外观</div>
                    <p class="mt-0.5 text-xs text-zinc-400">深色 / 浅色切换，跟随系统。</p>
                  </div>
                  <span
                    class="rounded border border-zinc-700/50 px-2 py-0.5 text-[10px] uppercase tracking-wider text-zinc-500"
                  >
                    即将推出
                  </span>
                </div>
                <div class="mx-4 border-t border-white/5">
                  <div class="flex items-center justify-between px-0 py-4 opacity-40">
                    <div class="max-w-md">
                      <div class="text-sm font-medium text-zinc-100">快捷键</div>
                      <p class="mt-0.5 text-xs text-zinc-400">发送请求、聚焦地址栏等全局快捷键。</p>
                    </div>
                    <span
                      class="rounded border border-zinc-700/50 px-2 py-0.5 text-[10px] uppercase tracking-wider text-zinc-500"
                    >
                      即将推出
                    </span>
                  </div>
                </div>
              </div>
            </section>

            <!-- 网络与代理 -->
            <section v-if="activeTab === 'network'">
              <header class="mb-6">
                <h2 class="text-lg font-semibold leading-tight text-zinc-100">网络与代理</h2>
                <p class="mt-1 text-xs text-zinc-500">配置全局 HTTP / SOCKS5 代理，应用于所有请求。</p>
              </header>

              <div class="rounded-xl border border-white/5 bg-zinc-900/50 p-1">
                <div class="flex items-center justify-between px-4 py-4">
                  <div class="max-w-md">
                    <div class="text-sm font-medium text-zinc-100">启用代理</div>
                    <p class="mt-0.5 text-xs text-zinc-400">
                      开启后所有请求经代理发出；关闭立即恢复直连，改动即保存。
                    </p>
                  </div>
                  <button
                    type="button"
                    role="switch"
                    :aria-checked="proxyEnabled"
                    class="relative h-[22px] w-[40px] shrink-0 rounded-full transition-colors duration-150"
                    :class="proxyEnabled ? 'bg-purple-500' : 'bg-white/10 border border-white/10'"
                    @click="toggleProxy"
                  >
                    <span
                      class="absolute top-1/2 h-[16px] w-[16px] -translate-y-1/2 rounded-full bg-white shadow transition-all duration-150"
                      :class="proxyEnabled ? 'left-[21px]' : 'left-[2px]'"
                    />
                  </button>
                </div>

                <div v-if="proxyEnabled" class="mx-4 border-t border-white/5">
                  <div class="flex items-center justify-between px-0 py-4">
                    <div class="max-w-md">
                      <div class="text-sm font-medium text-zinc-100">代理地址</div>
                      <p class="mt-0.5 text-xs text-zinc-400">
                        如 <code class="font-mono text-[11px]">http://127.0.0.1:7890</code> 或
                        <code class="font-mono text-[11px]">socks5://host:1080</code>；失焦自动保存。
                      </p>
                    </div>
                    <input
                      v-model="proxyUrl"
                      class="rf-input w-72 font-mono text-[12.5px]"
                      type="text"
                      placeholder="http://127.0.0.1:7890"
                      spellcheck="false"
                      @change="saveProxyUrl"
                    />
                  </div>

                  <div class="flex items-center justify-between border-t border-white/5 px-0 py-4">
                    <div class="max-w-md">
                      <div class="text-sm font-medium text-zinc-100">连通性测试</div>
                      <p class="mt-0.5 text-xs text-zinc-400">
                        {{ proxyTest ? proxyTest.message : '经当前代理请求一次公开目标，验证可用性。' }}
                      </p>
                    </div>
                    <button
                      class="rf-btn"
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
              <header class="mb-6">
                <h2 class="text-lg font-semibold leading-tight text-zinc-100">自增序列与变量</h2>
                <p class="mt-1 text-xs text-zinc-500">
                  请求中写 <code class="font-mono text-[11px]">&#123;&#123;$seq:key&#125;&#125;</code> 自动递增；
                  值即「下一次输出」，持久化、重启不丢。
                </p>
              </header>

              <div class="rounded-xl border border-white/5 bg-zinc-900/50 p-5">
                <div class="flex items-center justify-between pb-3">
                  <div class="text-sm font-medium text-zinc-100">序列列表</div>
                  <span class="text-[11px] text-zinc-500">改动失焦自动保存</span>
                </div>

                <div v-if="counters.length" class="overflow-hidden rounded-lg border border-white/5">
                  <table class="w-full border-collapse text-[12.5px]">
                    <thead>
                      <tr class="text-left text-[11px] text-zinc-500">
                        <th class="px-3 py-2 font-medium">Key</th>
                        <th class="w-28 px-3 py-2 font-medium">下一值</th>
                        <th class="w-32 px-3 py-2 text-right font-medium">操作</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr
                        v-for="c in counters"
                        :key="c.key || '__global__'"
                        class="border-t border-white/5"
                      >
                        <td class="px-3 py-2 font-mono text-zinc-100">
                          <span
                            class="mr-1.5 rounded bg-purple-500/15 px-1.5 py-px text-[10px] font-medium text-purple-400"
                          >
                            全局
                          </span>
                          <span v-if="!c.key">$seq</span>
                          <span v-else>{{ c.key }}</span>
                        </td>
                        <td class="px-3 py-2">
                          <input
                            v-model.number="c.value"
                            class="rf-input rf-input-sm w-20 text-right font-mono tabular-nums"
                            type="number"
                            min="1"
                            spellcheck="false"
                            @change="saveSeq(c)"
                          />
                        </td>
                        <td class="px-3 py-2 text-right">
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
                        </td>
                      </tr>
                    </tbody>
                  </table>
                </div>
                <p v-else class="m-0 py-2 text-[12.5px] text-zinc-500">暂无序列，可在下方新增。</p>

                <div class="mt-4 flex items-center gap-2 border-t border-white/5 pt-4">
                  <input
                    v-model="newSeqKey"
                    class="rf-input flex-1 font-mono text-[12.5px]"
                    type="text"
                    placeholder="序列 key（留空 = 全局 $seq）"
                    spellcheck="false"
                    @keydown.enter="addSeq"
                  />
                  <input
                    v-model.number="newSeqValue"
                    class="rf-input w-24 text-right font-mono tabular-nums"
                    type="number"
                    min="1"
                    placeholder="起始值"
                    spellcheck="false"
                    @keydown.enter="addSeq"
                  />
                  <button class="rf-btn rf-btn-primary" type="button" @click="addSeq">
                    <Icon name="plus" :size="13" />
                    添加新序列
                  </button>
                </div>
              </div>
            </section>

            <!-- 数据与备份 -->
            <section v-if="activeTab === 'data'">
              <header class="mb-6">
                <h2 class="text-lg font-semibold leading-tight text-zinc-100">数据与备份</h2>
                <p class="mt-1 text-xs text-zinc-500">导出当前项目备份，或从备份文件恢复。</p>
              </header>

              <div class="rounded-xl border border-white/5 bg-zinc-900/50 p-5">
                <div class="flex items-center gap-3">
                  <div
                    class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-purple-500/15 text-purple-400"
                  >
                    <Icon name="package" :size="18" />
                  </div>
                  <div class="min-w-0">
                    <div class="truncate text-sm font-medium text-zinc-100">
                      {{ project ? project.name : '未选择项目' }}
                    </div>
                    <p class="mt-0.5 truncate text-xs text-zinc-400">{{ projectSummary }}</p>
                  </div>
                </div>

                <div class="mt-4 flex items-center gap-2 border-t border-white/5 pt-4">
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
              <header class="mb-6">
                <h2 class="text-lg font-semibold leading-tight text-zinc-100">环境管理</h2>
                <p class="mt-1 text-xs text-zinc-500">
                  创建 / 编辑 / 删除环境变量；变量经
                  <code class="font-mono text-[11px]">&#123;&#123;变量&#125;&#125;</code>
                  注入请求，可在工作区顶部快速切换。
                </p>
              </header>

              <div class="rounded-xl border border-white/5 bg-zinc-900/50 p-1">
                <div class="flex items-center justify-between px-4 py-4">
                  <div class="max-w-md">
                    <div class="text-sm font-medium text-zinc-100">环境变量</div>
                    <p class="mt-0.5 text-xs text-zinc-400">管理不同环境的 Base URL 与变量集合。</p>
                  </div>
                  <button class="rf-btn" type="button" @click="showManager = true">
                    <Icon name="beaker" :size="13" />
                    打开环境管理
                  </button>
                </div>
              </div>
            </section>
          </div>
        </Transition>
      </div>
    </div>

    <EnvironmentManager v-model:open="showManager" />
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
</style>
