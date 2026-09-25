<script setup lang="ts">
/**
 * EnvironmentBar：环境选择器（工作区顶部栏右侧）。
 * - 触发框：状态色点 + 环境名；悬停 tooltip 展示完整 Base URL；
 * - 下拉结构：搜索框 → 分隔线 → 环境列表 → 分隔线 → 底部操作栏；
 * - 列表项（关键）：单卡片 flex 行，左区 [状态点 | 上下双行文字列]，
 *   双行 = 名称 / Base URL 各占一行（URL 永不与名称横向挤压截断）；
 *   选中态 accent 轻底 + 半透明边框，✓ 固定最右；hover 渐显复制/编辑；
 * - 底部：「+ 新建环境」「管理环境…」两端对齐；
 * - 👁️（环境变量速览）与下拉融为一个边线相连的组控（eb-group）。
 */
import { computed, nextTick, ref } from 'vue'
import { useWorkspaceStore } from '../stores/workspace'
import { useLocaleStore } from '../stores/locale'
import { useToast } from '../composables/useToast'
import { copyText } from '../utils/clipboard'
import { envBaseUrl, envColorClass } from '../utils/environment'
import CustomSelect from './ui/CustomSelect.vue'
import EnvironmentQuickView from './EnvironmentQuickView.vue'
import Icon from './ui/Icon.vue'
import IconButton from './ui/IconButton.vue'
import Tooltip from './ui/Tooltip.vue'
import { lazyComponent } from '../composables/lazyComponent'
import type { Environment } from '../types/foxApi'

// 环境管理大弹窗（1400+ 行）挂在顶栏常驻组件上会把它的 chunk 拖进首屏：
// 异步化后首次打开环境管理才拉取，顶栏不再为一个低频弹窗买单。
const EnvironmentManager = lazyComponent(() => import('./EnvironmentManager.vue'))

const store = useWorkspaceStore()
const locale = useLocaleStore()
const t = locale.t
const toast = useToast()

const barEl = ref<HTMLElement | null>(null)
const selectRef = ref<InstanceType<typeof CustomSelect> | null>(null)
const searchInputEl = ref<HTMLInputElement | null>(null)
const showQuick = ref(false)
const showManager = ref(false)
/** 环境管理弹窗定位：打开时聚焦的环境 id（null=跟随当前激活环境）。 */
const managerEnvId = ref<string | null>(null)
/** 打开管理弹窗时是否立即新建一条环境（底部「+ 新建环境」）。 */
const managerCreate = ref(false)
/** 环境下拉菜单是否打开：打开时禁用触发器上的悬浮提示，避免与菜单重叠。 */
const envMenuOpen = ref(false)
/** 下拉顶部搜索关键词（按环境名 / Base URL 过滤）。 */
const searchQuery = ref('')

const activeEnv = computed(
  () => store.environments.find((e) => e.id === store.activeEnvId) ?? null,
)

const options = computed(() => [
  { value: '', label: t('envbar.noEnv') },
  ...store.environments.map((env) => ({ value: env.id, label: env.name })),
])

/** 过滤后的选项：有关键词时剔除「无环境」，按名称 / Base URL 不分大小写匹配。 */
const filteredOptions = computed(() => {
  const q = searchQuery.value.trim().toLowerCase()
  if (!q) return options.value
  return options.value.filter((o) => {
    if (o.value === '') return false
    const url = envBaseUrl(envByValue(o.value), store.project?.id).toLowerCase()
    return o.label.toLowerCase().includes(q) || url.includes(q)
  })
})

/** 搜索无结果（有关键词且过滤列表为空）。 */
const noMatch = computed(
  () => !!searchQuery.value.trim() && filteredOptions.value.length === 0,
)

/** 悬停 tooltip：完整 Base URL（无环境 / 未配置时不显示）。 */
const tooltipContent = computed(() =>
  activeEnv.value ? envBaseUrl(activeEnv.value, store.project?.id) : '',
)

function envByValue(value: string | number): Environment | null | undefined {
  if (value === '' || value == null) return null
  return store.environments.find((e) => e.id === String(value))
}

/** 选项对应解析后的 Base URL（无环境 / 未配置为空串）。 */
function urlOf(value: string | number): string {
  return envBaseUrl(envByValue(value), store.project?.id)
}

function onChange(value: string | number): void {
  void store.setEnvironment(value === '' ? null : String(value))
}

function colorClass(name: string): string {
  return envColorClass(name)
}

/** 下拉打开：清空上次搜索并聚焦搜索框。 */
async function onMenuOpen(): Promise<void> {
  envMenuOpen.value = true
  searchQuery.value = ''
  await nextTick()
  searchInputEl.value?.focus()
}

function onMenuClose(): void {
  envMenuOpen.value = false
  searchQuery.value = ''
}

/** 搜索框 Enter：选中首个匹配项并收起。 */
function onSearchEnter(): void {
  if (!searchQuery.value.trim()) return
  const first = filteredOptions.value[0]
  if (!first) return
  onChange(first.value)
  selectRef.value?.close()
}

/** 搜索框 Esc：有关键词先清空，再按一次收起整个下拉。 */
function onSearchEsc(): void {
  if (searchQuery.value) {
    searchQuery.value = ''
    return
  }
  selectRef.value?.close()
}

/** 快捷复制环境 Base URL（按钮点击已 .stop，不会触发选项选中）。 */
async function copyEnvUrl(value: string | number): Promise<void> {
  const url = urlOf(value)
  if (!url) return
  const ok = await copyText(url)
  if (ok) toast.success(t('editor.baseCopied'))
  else toast.error(t('response.copyFail'))
}

/** 打开环境管理：先收起下拉，再带定位（编辑指定环境 / 新建 / 全局入口）。 */
function openManager(envId: string | null = null, create = false): void {
  selectRef.value?.close()
  managerEnvId.value = envId
  managerCreate.value = create
  showManager.value = true
}

function onManagerOpenChange(open: boolean): void {
  showManager.value = open
  if (!open) {
    managerEnvId.value = null
    managerCreate.value = false
  }
}
</script>

<template>
  <div ref="barEl" class="env-bar">
    <div class="eb-group">
      <Tooltip :content="tooltipContent" :disabled="envMenuOpen" placement="bottom">
        <CustomSelect
          ref="selectRef"
          class="eb-select"
          pop-class="env-pop"
          :model-value="store.activeEnvId ?? ''"
          :options="filteredOptions"
          :placeholder="t('envbar.placeholder')"
          size="sm"
          :pop-min-width="320"
          @change="onChange"
          @open="onMenuOpen"
          @close="onMenuClose"
        >
          <template #display>
            <span class="edot" :class="`ed-${colorClass(activeEnv?.name ?? '')}`"></span>
            <span class="env-display-name">{{ activeEnv?.name ?? t('envbar.noEnv') }}</span>
          </template>
          <template #search>
            <div class="eb-search">
              <div class="eb-search-row">
                <Icon class="eb-search-icon" name="search" :size="13" />
                <input
                  ref="searchInputEl"
                  v-model="searchQuery"
                  type="text"
                  class="eb-search-input"
                  :placeholder="t('envbar.search')"
                  @keydown.enter.prevent="onSearchEnter"
                  @keydown.esc.stop="onSearchEsc"
                />
              </div>
              <div v-if="noMatch" class="eb-search-empty">{{ t('envbar.noMatch') }}</div>
            </div>
          </template>
          <template #option="{ option }">
            <div class="env-opt-left">
              <span class="env-dot" :class="`ed-${colorClass(option.label)}`"></span>
              <div class="env-opt-text">
                <span class="env-opt-name" :title="option.label">{{ option.label }}</span>
                <span v-if="urlOf(option.value)" class="env-opt-url" :title="urlOf(option.value)">
                  {{ urlOf(option.value) }}
                </span>
              </div>
            </div>
          </template>
          <template #actions="{ option }">
            <span v-if="option.value !== ''" class="eb-actions">
              <button
                v-if="urlOf(option.value)"
                type="button"
                class="env-act"
                :title="t('envbar.copyUrl')"
                :aria-label="t('envbar.copyUrl')"
                @click.stop="copyEnvUrl(option.value)"
              >
                <Icon name="copy" :size="12" />
              </button>
              <button
                type="button"
                class="env-act"
                :title="t('envbar.editEnv')"
                :aria-label="t('envbar.editEnv')"
                @click.stop="openManager(String(option.value))"
              >
                <Icon name="pencil" :size="12" />
              </button>
            </span>
          </template>
          <template #footer>
            <div class="env-pop-footer">
              <button type="button" class="env-foot-btn" @click.stop="openManager(null, true)">
                <Icon name="plus" :size="12" />
                <span>{{ t('envmgr.addEnv') }}</span>
              </button>
              <button type="button" class="env-foot-btn" @click.stop="openManager()">
                <Icon name="settings" :size="12" />
                <span>{{ t('envbar.manage') }}</span>
              </button>
            </div>
          </template>
        </CustomSelect>
      </Tooltip>
      <span class="eb-sep" aria-hidden="true"></span>
      <Tooltip :content="activeEnv ? t('envbar.viewVars') : t('envbar.noActiveEnv')" placement="bottom">
        <IconButton
          class="eb-eye"
          name="eye"
          :size="14"
          :label="activeEnv ? t('envbar.viewVars') : t('envbar.noActiveEnv')"
          :disabled="!activeEnv"
          @click="showQuick = !showQuick"
        />
      </Tooltip>
    </div>

    <EnvironmentQuickView
      v-if="showQuick && activeEnv"
      :anchor="barEl"
      @close="showQuick = false"
      @manage="showQuick = false; openManager()"
    />
    <EnvironmentManager
      :open="showManager"
      :initial-env-id="managerEnvId"
      :create-new="managerCreate"
      @update:open="onManagerOpenChange"
    />
  </div>
</template>

<style scoped>
.env-bar {
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
}

/* ---- 统一 32px Pill 组控：[下拉 | 👁️] ---- */
.eb-group {
  display: inline-flex;
  align-items: center;
  height: 32px;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: var(--bg-hover);
  overflow: hidden;
  transition:
    border-color var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}
.eb-group:hover {
  border-color: var(--border-strong);
}
.eb-group:focus-within {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-tint);
}

.eb-select {
  width: 168px;
  flex-shrink: 0;
}
.eb-select :deep(.cs-trigger) {
  height: 32px;
  gap: 8px;
  border: none;
  background: transparent;
  border-radius: 0;
  font-family: var(--font-ui);
  font-weight: 500;
}
/* 下拉箭头弱化：更小、更低透明度（点击区仍是整行） */
.eb-select :deep(.cs-caret) {
  opacity: 0.6;
  transform: scale(0.85);
}
.eb-select :deep(.cs-trigger:hover:not(:disabled)) {
  background: var(--bg-hover);
}
.eb-select :deep(.cs-trigger:focus-visible) {
  outline: none;
}
.eb-select :deep(.cs.open .cs-trigger) {
  border: none;
  box-shadow: none;
}

.eb-sep {
  width: 1px;
  height: 18px;
  flex-shrink: 0;
  background: var(--border);
}

.eb-eye {
  width: 30px;
  height: 30px;
  border-radius: 0;
  background: transparent;
}
.eb-eye:hover:not(:disabled) {
  background: transparent;
}

/* ---- 环境色点（映射 utils/environment.ts 的 envColorClass） ----
   6px 小点 + currentColor 同色柔光，低调指示而非视觉主角 */
.edot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
  background: currentColor;
  color: var(--text-3);
  box-shadow: 0 0 6px color-mix(in srgb, currentColor 45%, transparent);
}

/* ---- 触发区展示：仅 状态点 + 环境名 ---- */
.env-display-name {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-1);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ---- 下拉顶部搜索 ----
   低调 inset 灰底（text-1 6% 双主题自适应），聚焦不换底只换边——无亮度跳变 */
.eb-search-row {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 4px 8px;
  border: 1px solid transparent;
  border-radius: var(--radius-md);
  background: color-mix(in srgb, var(--text-1) 6%, transparent);
  transition: border-color var(--dur) var(--ease);
}
.eb-search-row:focus-within {
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
}
.eb-search-icon {
  flex-shrink: 0;
  width: 13px;
  height: 13px;
  color: var(--text-3);
}
.eb-search-input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font-family: var(--font-ui);
  font-size: 12px;
  color: var(--text-1);
}
.eb-search-input::placeholder {
  font-size: 12px;
  color: var(--text-3);
}
.eb-search-empty {
  padding: 10px 8px 2px;
  text-align: center;
  font-size: 11.5px;
  color: var(--text-3);
}

/* ---- 列表项左区：[状态圆点 | 上下双行文字列] ----
   双行结构（回归锁定见 EnvironmentBar.spec）：
   .env-opt-left > .env-dot + .env-opt-text(.flex-col) > name / url
   —— URL 独占第二行，只在自己行内 truncate，不与名称横向抢宽 */
.env-opt-left {
  order: 1;
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  flex: 1;
}
.env-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
  background: currentColor;
  color: var(--text-3);
  box-shadow: 0 0 6px color-mix(in srgb, currentColor 45%, transparent);
}
.env-opt-text {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
  flex: 1;
}
.env-opt-name {
  font-size: 12px;
  font-weight: 500;
  line-height: 1.3;
  color: var(--text-1);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* URL 次级降噪：muted 灰 + mono，不与名称争视觉焦点 */
.env-opt-url {
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1.3;
  color: var(--text-3);
  min-width: 0;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ---- 选项右侧 hover 快捷操作（复制 URL / 编辑）：默认隐身占位，hover/高亮浮现 ---- */
.eb-actions {
  order: 2;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  opacity: 0;
  transition: opacity var(--dur) var(--ease);
}
.env-act {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  padding: 0;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
}
.env-act:hover {
  background: color-mix(in srgb, var(--text-1) 8%, transparent);
  color: var(--text-1);
}

/* ---- 底部固定操作栏：上分隔线 + 两端对齐低调浅灰按钮 ---- */
.env-pop-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  padding: 2px 4px;
}
.env-foot-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 6px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  font-family: var(--font-ui);
  font-size: 12px;
  color: var(--text-3);
  cursor: pointer;
  transition:
    background var(--dur) var(--ease),
    color var(--dur) var(--ease);
}
.env-foot-btn:hover {
  background: var(--bg-hover);
  color: var(--text-1);
}

/* ⚠ popClass 挂在 .cs-pop 自身（同元素双类），必须用 .cs-pop.env-pop，
   写成后代 .env-pop .cs-pop 永不匹配（曾致整套样式静默失效）。 */

/* ---- 浮层容器 ----
   结构：[搜索 → 分隔线] → [列表] → [分隔线 → 底栏]；宽 320、rounded-xl、深投影 */
:global(.cs-pop.env-pop) {
  min-width: 320px;
  max-width: min(360px, calc(100vw - 16px));
  padding: 6px;
  border-radius: var(--radius-xl);
  border-color: var(--border-strong);
  background: var(--bg-elevated);
  box-shadow:
    0 24px 48px -12px rgb(0 0 0 / 0.5),
    0 8px 24px -8px rgb(0 0 0 / 0.4);
}
/* 卡片行：全行 transparent 边框占位（border-box 等高，选中换 accent 边不撑高） */
:global(.cs-pop.env-pop .cs-opt) {
  height: auto;
  min-height: 34px;
  padding: 6px 8px;
  border: 1px solid transparent;
  border-radius: var(--radius-md);
  gap: 6px;
  font-family: var(--font-ui);
  white-space: normal;
}
/* 未选中 hover：背景微亮（--bg-hover 深=白4% / 浅=黑4%） */
:global(.cs-pop.env-pop .cs-opt.hl) {
  background: var(--bg-hover);
}
/* 选中态：accent 半透明轻底 + accent/30% 边框，✓ accent */
:global(.cs-pop.env-pop .cs-opt.sel) {
  color: var(--text-1);
  background: var(--accent-tint);
  border-color: color-mix(in srgb, var(--accent) 30%, transparent);
}
/* 选中行再 hover：保 tint 不被 hl 灰底盖掉，边框略加深 */
:global(.cs-pop.env-pop .cs-opt.hl.sel) {
  background: var(--accent-tint);
  border-color: color-mix(in srgb, var(--accent) 45%, transparent);
}
:global(.cs-pop.env-pop .cs-opt.sel .cs-opt-check) {
  color: var(--accent);
}
/* 布局序：左区内容(1) → hover 操作(2) → 最右 ✓(3)，状态点在左区行首，互不重叠；
   ✓ 空位也占 16px，未选中行右缘对齐 */
:global(.cs-pop.env-pop .cs-opt-label) {
  order: 1;
  white-space: normal;
  overflow: visible;
  line-height: 1.4;
}
:global(.cs-pop.env-pop .cs-opt-check) {
  order: 3;
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  margin-left: 4px;
}
:global(.cs-pop.env-pop .cs-opt:hover .eb-actions),
:global(.cs-pop.env-pop .cs-opt.hl .eb-actions) {
  opacity: 1;
}

/* ---- 环境语义色（映射 utils/environment.ts 的 envColorClass）----
   ⚠ 必须排在 .edot / .env-dot 之后：同特异类靠样式表顺序决胜，
   放前面会被 .env-dot 的 text-3 默认色覆盖（列表点全灰踩过坑）。 */
.ed-dev {
  color: var(--success);
}
.ed-test {
  color: var(--info);
}
.ed-staging {
  color: var(--warning);
}
.ed-prod {
  color: var(--orange);
}
.ed-global {
  color: var(--accent);
}
</style>