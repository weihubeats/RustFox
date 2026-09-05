<script setup lang="ts">
/**
 * JsonEditor：JSON 编辑区（暗色代码编辑器风格）。
 * - 覆盖层方案：透明 textarea 叠加在高亮 <pre> 上（零依赖，滚动同步）；
 * - 左侧行号栏：随内容垂直平移、细边框与正文分隔；
 * - 顶部工具栏（非悬浮）：左侧校验状态 Tag，右侧 美化 / 压缩 / 查找 / 复制 按钮；
 *   编辑区内没有任何绝对定位浮层遮挡代码。
 * - 深色底色 #121318，聚焦时 1px 紫色光晕（原 3px 重描边移除）。
 */
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import { useToast } from '../../composables/useToast'
import { useLocaleStore } from '../../stores/locale'
import { copyText } from '../../utils/clipboard'
import { highlightJSON, highlightJSONText } from '../../utils/highlight'
import { compactJson, prettyJson } from '../../utils/jsonFormat'
import { JsonFormatError } from '../../utils/jsonFormat'
import Icon from './Icon.vue'

const props = withDefaults(
  defineProps<{
    modelValue: string
    placeholder?: string
    /** 编辑区最小高度（px）。 */
    minHeight?: number
    /** 查找词（实时/防抖搜索匹配并高亮）。 */
    query?: string
    /** 当前选中的匹配索引（0-based）。 */
    activeMatch?: number
  }>(),
  { placeholder: '', minHeight: 120, query: '', activeMatch: 0 },
)

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void
  (e: 'match-count', total: number): void
}>()

const toast = useToast()
const locale = useLocaleStore()
const t = locale.t
const taRef = ref<HTMLTextAreaElement | null>(null)
const preRef = ref<HTMLElement | null>(null)
const scrollTop = ref(0)

/**
 * 大内容保护：超过阈值后停用逐键语法高亮 / JSON 校验 / 行号渲染。
 * 这些计算随内容大小线性放大（每次键入全文 parse + 正则高亮 + v-html 重建
 * + 每行一个行号 div），大文档下会造成明显卡顿与内存高水位，故退化为
 * 纯文本编辑（textarea 直接着色，无覆盖层）。
 */
const LARGE_DOC_CHARS = 200_000

const isLargeDoc = computed(() => props.modelValue.length > LARGE_DOC_CHARS)

/**
 * 高亮/校验用文本：小文档（<20k）即时跟手；中等以上防抖 150ms。
 * 原来每键全量 `highlightJSON + split + JSON.parse`（高亮/行号/校验各一次），
 * 百 KB 文档下每次键入三遍全量扫描；输入本身走 textarea 非受控即时响应，
 * 防抖只延迟着色与状态 Tag，不影响键入手感。
 */
const SHOWN_DEBOUNCE_CHARS = 20_000
const shownText = ref(props.modelValue)
let shownTimer: ReturnType<typeof setTimeout> | undefined
watch(
  () => props.modelValue,
  (v) => {
    if (shownTimer) clearTimeout(shownTimer)
    if (v.length < SHOWN_DEBOUNCE_CHARS) {
      shownText.value = v
      return
    }
    shownTimer = setTimeout(() => {
      shownText.value = v
    }, 150)
  },
)
onUnmounted(() => {
  if (shownTimer) clearTimeout(shownTimer)
})

/** 空内容渲染一个空格，保证 pre 与 textarea 高度一致（滚动同步前提）。 */
const matchCount = computed(() => {
  if (!props.query || isLargeDoc.value) return 0
  const ql = props.query.toLowerCase()
  const lower = props.modelValue.toLowerCase()
  let n = 0
  let from = 0
  for (;;) {
    const idx = lower.indexOf(ql, from)
    if (idx === -1) break
    n += 1
    from = idx + ql.length
  }
  return n
})

watch(
  matchCount,
  (cnt) => {
    emit('match-count', cnt)
  },
  { immediate: true },
)

/** 空内容渲染一个空格，保证 pre 与 textarea 高度一致（滚动同步前提）。 */
const html = computed(() => {
  if (isLargeDoc.value) return ''
  const val = props.modelValue.length ? props.modelValue : ' '
  if (props.query) {
    return highlightJSONText(val, props.query, props.activeMatch)
  }
  return highlightJSON(val)
})

function getMatchOffsets(text: string, q: string, targetIdx: number): [number, number] | null {
  if (!q) return null
  const ql = q.toLowerCase()
  const lower = text.toLowerCase()
  let from = 0
  let cur = 0
  for (;;) {
    const idx = lower.indexOf(ql, from)
    if (idx === -1) return null
    if (cur === targetIdx) {
      return [idx, idx + q.length]
    }
    cur += 1
    from = idx + ql.length
  }
}

watch(
  () => [props.query, props.activeMatch],
  ([q, matchIdx]) => {
    if (!q || !taRef.value || matchIdx === undefined) return
    const offsets = getMatchOffsets(props.modelValue, String(q), Number(matchIdx))
    if (!offsets) return
    const [start, end] = offsets
    taRef.value.setSelectionRange(start, end)
    const linesBefore = props.modelValue.slice(0, start).split('\n').length - 1
    const lineHeight = 19.4
    const targetScroll = Math.max(0, linesBefore * lineHeight - 40)
    taRef.value.scrollTop = targetScroll
    if (preRef.value) {
      preRef.value.scrollTop = targetScroll
    }
  },

)

const lineCount = computed(() => (isLargeDoc.value ? 0 : shownText.value.split('\n').length))

/** 行号栏宽度随位数增长：左留白 + 位数×字宽 + 右留白。 */
const gutterWidth = computed(() =>
  isLargeDoc.value ? 0 : 10 + String(lineCount.value).length * 8 + 10,
)

const status = computed<'empty' | 'ok' | 'invalid' | 'large'>(() => {
  if (isLargeDoc.value) return 'large'
  const t = shownText.value.trim()
  if (!t) return 'empty'
  try {
    JSON.parse(t)
    return 'ok'
  } catch {
    return 'invalid'
  }
})

/** 工具栏状态 Tag 文案与图标（empty 时不渲染）。 */
const statusText = computed(
  () =>
    ({ ok: t('jsonEditor.ok'), invalid: t('jsonEditor.invalid'), large: t('jsonEditor.large') })[
      status.value as 'ok' | 'invalid' | 'large'
    ] ?? '',
)
const statusIcon = computed(() => (status.value === 'ok' ? 'check' : 'x'))

/** 是否有内容可格式化 / 复制（textarea 实时值为权威，见 format 注释）。 */
const hasContent = computed(() => (taRef.value?.value ?? props.modelValue).trim().length > 0)

function onInput(e: Event): void {
  emit('update:modelValue', (e.target as HTMLTextAreaElement).value)
}

function syncScroll(e: Event): void {
  const ta = e.target as HTMLTextAreaElement
  scrollTop.value = ta.scrollTop
  if (preRef.value) {
    preRef.value.scrollTop = ta.scrollTop
    preRef.value.scrollLeft = ta.scrollLeft
  }
}

// 内容变化（格式化 / 粘贴 / 回退 / 外部保存）后重新对齐滚动与 DOM 同步，避免高亮层与行号错位。
watch(
  () => props.modelValue,
  (newVal) => {
    if (taRef.value && taRef.value.value !== newVal) {
      taRef.value.value = newVal
    }
    if (taRef.value && preRef.value) {
      preRef.value.scrollTop = taRef.value.scrollTop
      preRef.value.scrollLeft = taRef.value.scrollLeft
      scrollTop.value = taRef.value.scrollTop
    }
  },
)

function format(mode: 'pretty' | 'compact'): void {
  // 以 textarea 的实时 DOM 值为权威来源：真实浏览器里 input 事件可能丢失或
  // 延迟（拖拽写入、自动填充、受控回写竞态），导致 props.modelValue 落后于
  // 用户实际编辑的内容——此时若按 props 格式化会把编辑器回退成旧数据。
  const text = (taRef.value?.value ?? props.modelValue).trim()
  if (!text) return
  try {
    // 无损格式化：保留重复键、键顺序与数字原文（parse/stringify 往返会丢重复键）。
    const out = mode === 'compact' ? compactJson(text) : prettyJson(text)
    if (out !== props.modelValue) {
      emit('update:modelValue', out)
    } else if (taRef.value && taRef.value.value !== out) {
      // 模型已一致但 DOM 残留未同步的值：直接纠正 DOM。
      taRef.value.value = out
    }
    toast.success(mode === 'compact' ? t('jsonEditor.compacted') : t('jsonEditor.prettified'))
    void nextTick(() => {
      // 双保险：emit 回写后强制对齐 DOM（WebKit 偶发不回写受控 value）。
      if (taRef.value && taRef.value.value !== props.modelValue) {
        taRef.value.value = props.modelValue
      }
      taRef.value?.focus()
    })
  } catch (err) {
    toast.error(err instanceof JsonFormatError ? t('jsonEditor.invalidJson', { v: err.message }) : t('jsonEditor.invalidJsonPlain'))
  }
}

async function copyJson(): Promise<void> {
  const text = taRef.value?.value ?? props.modelValue
  if (!text.trim()) return
  const ok = await copyText(text)
  if (ok) toast.success(t('jsonEditor.copied'))
  else toast.error(t('response.copyFail'))
}
</script>

<template>
  <div class="json-editor">
    <!-- 顶部工具栏：状态 Tag + 快捷操作（替代原悬浮层，不遮挡代码） -->
    <div class="je-toolbar">
      <span
        v-if="status !== 'empty'"
        class="je-status"
        :class="status"
        :title="
          status === 'invalid'
            ? t('jsonEditor.invalidHint')
            : status === 'large'
              ? t('jsonEditor.largeHint')
              : t('jsonEditor.okHint')
        "
      >
        <Icon v-if="status === 'ok' || status === 'invalid'" :name="statusIcon" :size="11" />
        <span v-else class="je-dot" aria-hidden="true"></span>
        {{ statusText }}
      </span>
      <span v-else class="je-toolbar-spacer" aria-hidden="true"></span>
      <div class="je-actions">
        <button
          class="je-btn"
          type="button"
          :title="t('jsonEditor.prettyHint')"
          :disabled="!hasContent"
          @click="format('pretty')"
        >
          <Icon name="zap" :size="12" />
        </button>
        <button
          class="je-btn"
          type="button"
          :title="t('jsonEditor.compactHint')"
          :disabled="!hasContent"
          @click="format('compact')"
        >
          <Icon name="minimize-2" :size="12" />
        </button>
        <button class="je-btn" type="button" :title="t('jsonEditor.copyHint')" :disabled="!hasContent" @click="copyJson">
          <Icon name="copy" :size="12" />
        </button>
      </div>
    </div>

    <div class="hl-wrap" :style="{ minHeight: `${minHeight}px` }">
      <div
        v-if="!isLargeDoc"
        class="hl-gutter"
        :style="{ width: `${gutterWidth}px` }"
        aria-hidden="true"
      >
        <div
          class="hl-gutter-inner"
          :style="{ transform: `translateY(${-scrollTop}px)` }"
        >
          <div v-for="n in lineCount" :key="n" class="hl-gutter-line">{{ n }}</div>
        </div>
      </div>
      <pre
        v-if="!isLargeDoc"
        ref="preRef"
        class="hl-pre"
        aria-hidden="true"
        v-html="html"
        :style="{ paddingLeft: `${gutterWidth}px` }"
      ></pre>
      <textarea
        ref="taRef"
        class="hl-ta"
        :class="{ plain: isLargeDoc }"
        :value="modelValue"
        :placeholder="placeholder"
        spellcheck="false"
        :style="{ paddingLeft: `${gutterWidth}px` }"
        @input="onInput"
        @change="onInput"
        @scroll="syncScroll"
      ></textarea>
    </div>
  </div>
</template>

<style scoped>
.json-editor {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1;
  min-height: 0;
}

.hl-wrap {
  position: relative;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--code-bg);
  overflow: hidden;
  flex: 1;
  min-height: 0;
  transition:
    border-color var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}

/* 1px 紫色聚焦光晕（替代原 3px 重描边） */
.hl-wrap:focus-within {
  border-color: rgba(168, 85, 247, 0.6);
  box-shadow: 0 0 0 1px rgba(168, 85, 247, 0.5);
}

.hl-pre,
.hl-ta {
  margin: 0;
  padding: 10px 12px;
  font-family: var(--font-mono);
  font-size: 12.5px;
  line-height: 1.55;
  white-space: pre;
  tab-size: 2;
}

.hl-pre {
  position: absolute;
  inset: 0;
  color: var(--code-fg);
  pointer-events: none;
  overflow: hidden;
  word-break: normal;
}

.hl-ta {
  position: relative;
  width: 100%;
  height: 100%;
  display: block;
  box-sizing: border-box;
  resize: vertical;
  border: none;
  outline: none;
  background: transparent;
  color: transparent;
  caret-color: var(--code-caret);
  overflow: auto;
}
.hl-ta::placeholder {
  color: var(--text-3);
}

/* 纯文本模式：大文档降级，直接在 textarea 上着色 */
.hl-ta.plain {
  color: var(--text-1);
  background: var(--code-bg);
}

/* ---- 语法高亮色系 ---- */
:deep(.hl-k) {
  color: var(--tok-key, #e06c75);
}
:deep(.hl-s) {
  color: var(--tok-str, #98c379);
}
:deep(.hl-n) {
  color: var(--tok-num, #d19a66);
}
:deep(.hl-b) {
  color: var(--tok-bool, #56b6c2);
}
:deep(.hl-null) {
  color: var(--tok-null, #56b6c2);
  font-style: italic;
}
:deep(.hl-p) {
  color: var(--tok-punct, #abb2bf);
}
:deep(.hl-c) {
  color: var(--tok-gutter, #5c6370);
  font-style: italic;
}

/* ---- 查找标记 ---- */
:deep(.rp-find-mark) {
  background: var(--accent-tint, rgba(99, 102, 241, 0.25));
  color: inherit;
  border-radius: 2px;
  padding: 0 1px;
}
:deep(.rp-find-mark.active) {
  background: var(--accent, #a855f7);
  color: #fff;
  outline: 1px solid var(--accent, #a855f7);
  outline-offset: 1px;
}

/* 行号栏 */
.hl-gutter {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  background: var(--bg-card, #121318);
  border-right: 1px solid var(--border, rgba(255, 255, 255, 0.08));
  overflow: hidden;
  user-select: none;
  pointer-events: none;
  z-index: 1;
}
.hl-gutter-inner {
  padding: 10px 0;
  will-change: transform;
}
.hl-gutter-line {
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1.55;
  color: var(--tok-gutter, #5c6370);
  text-align: right;
  padding-right: 8px;
}

/* 工具栏 */
.je-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 2px 2px 4px;
}
.je-toolbar-spacer {
  flex: 1;
}

.je-status {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 2px 7px;
  border-radius: 9999px;
  font-size: 11px;
  font-weight: 500;
  line-height: 1.4;
  letter-spacing: -0.01em;
}
.je-status.ok {
  background: var(--success-tint);
  color: var(--success);
}
.je-status.invalid {
  background: var(--danger-tint);
  color: var(--danger);
}
.je-status.large {
  background: var(--warning-tint);
  color: var(--warning);
}

.je-dot {
  width: 5px;
  height: 5px;
  border-radius: 9999px;
  background: currentColor;
}

.je-actions {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  margin-left: auto;
}

.je-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: 1px solid transparent;
  border-radius: 5px;
  background: transparent;
  color: var(--text-3);
  cursor: pointer;
  transition:
    background var(--dur) var(--ease),
    border-color var(--dur) var(--ease),
    color var(--dur) var(--ease);
}
.je-btn:hover:not(:disabled) {
  background: var(--bg-hover);
  border-color: var(--border);
  color: var(--text-1);
}
.je-btn.active {
  background: var(--accent-tint, rgba(168, 85, 247, 0.15));
  color: var(--accent, #a855f7);
  border-color: var(--accent, #a855f7);
}
.je-btn:disabled {
  opacity: 0.35;
  cursor: default;
}
</style>
