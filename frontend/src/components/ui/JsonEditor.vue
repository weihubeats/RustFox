<script setup lang="ts">
/**
 * JsonEditor：JSON 编辑区（暗色代码编辑器风格）。
 * - 覆盖层方案：透明 textarea 叠加在高亮 <pre> 上（零依赖，滚动同步）；
 * - 左侧行号栏：随内容垂直平移、细边框与正文分隔；
 * - 顶部工具栏（非悬浮）：左侧校验状态 Tag，右侧 美化 / 压缩 / 复制 按钮；
 *   编辑区内没有任何绝对定位浮层遮挡代码。
 * - 深色底色 #121318，聚焦时 1px 紫色光晕（原 3px 重描边移除）。
 */
import { computed, nextTick, ref, watch } from 'vue'
import { useToast } from '../../composables/useToast'
import { copyText } from '../../utils/clipboard'
import { highlightJSON } from '../../utils/highlight'
import { compactJson, prettyJson } from '../../utils/jsonFormat'
import { JsonFormatError } from '../../utils/jsonFormat'
import Icon from './Icon.vue'

const props = withDefaults(
  defineProps<{
    modelValue: string
    placeholder?: string
    /** 编辑区最小高度（px）。 */
    minHeight?: number
  }>(),
  { placeholder: '', minHeight: 120 },
)

const emit = defineEmits<{ (e: 'update:modelValue', value: string): void }>()

const toast = useToast()
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

/** 空内容渲染一个空格，保证 pre 与 textarea 高度一致（滚动同步前提）。 */
const html = computed(() =>
  isLargeDoc.value ? '' : highlightJSON(props.modelValue.length ? props.modelValue : ' '),
)

const lineCount = computed(() => (isLargeDoc.value ? 0 : props.modelValue.split('\n').length))

/** 行号栏宽度随位数增长：左留白 + 位数×字宽 + 右留白。 */
const gutterWidth = computed(() =>
  isLargeDoc.value ? 0 : 10 + String(lineCount.value).length * 8 + 10,
)

const status = computed<'empty' | 'ok' | 'invalid' | 'large'>(() => {
  if (isLargeDoc.value) return 'large'
  const t = props.modelValue.trim()
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
    ({ ok: 'JSON 有效', invalid: '语法错误', large: '内容过大，高亮已停用' })[
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

// 内容变化（格式化 / 粘贴 / 回退）后重新对齐滚动，避免高亮层与行号错位。
watch(
  () => props.modelValue,
  () => {
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
  const t = (taRef.value?.value ?? props.modelValue).trim()
  if (!t) return
  try {
    // 无损格式化：保留重复键、键顺序与数字原文（parse/stringify 往返会丢重复键）。
    const out = mode === 'compact' ? compactJson(t) : prettyJson(t)
    if (out !== props.modelValue) {
      emit('update:modelValue', out)
    } else if (taRef.value && taRef.value.value !== out) {
      // 模型已一致但 DOM 残留未同步的值：直接纠正 DOM。
      taRef.value.value = out
    }
    toast.success(mode === 'compact' ? '已压缩' : '已美化')
    void nextTick(() => {
      // 双保险：emit 回写后强制对齐 DOM（WebKit 偶发不回写受控 value）。
      if (taRef.value && taRef.value.value !== props.modelValue) {
        taRef.value.value = props.modelValue
      }
      taRef.value?.focus()
    })
  } catch (err) {
    toast.error(err instanceof JsonFormatError ? `JSON 无效：${err.message}` : 'JSON 无效，无法格式化')
  }
}

async function copyJson(): Promise<void> {
  const t = taRef.value?.value ?? props.modelValue
  if (!t.trim()) return
  const ok = await copyText(t)
  if (ok) toast.success('JSON 已复制')
  else toast.error('复制失败，请手动选择文本')
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
            ? 'JSON 语法错误，请检查后重试'
            : status === 'large'
              ? '内容超过 200k 字符，语法高亮与校验已停用'
              : 'JSON 语法有效'
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
          title="美化（格式化 JSON）"
          :disabled="!hasContent"
          @click="format('pretty')"
        >
          <Icon name="zap" :size="12" />
        </button>
        <button
          class="je-btn"
          type="button"
          title="压缩 JSON"
          :disabled="!hasContent"
          @click="format('compact')"
        >
          <Icon name="minimize-2" :size="12" />
        </button>
        <button class="je-btn" type="button" title="复制 JSON" :disabled="!hasContent" @click="copyJson">
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
  border-radius: 8px;
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
  color: var(--code-placeholder);
}

/* 大内容模式：无高亮覆盖层，textarea 直接着色 */
.hl-ta.plain {
  color: var(--code-fg);
}
.hl-ta::selection {
  background: rgba(168, 85, 247, 0.28);
}

/* ---- 行号栏 ---- */
.hl-gutter {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  z-index: 1;
  background: var(--bg-hover);
  border-right: 1px solid var(--border);
  user-select: none;
  pointer-events: none;
}

.hl-gutter-inner {
  padding-top: 10px;
  will-change: transform;
}

.hl-gutter-line {
  height: calc(12.5px * 1.55);
  line-height: calc(12.5px * 1.55);
  text-align: right;
  padding-right: 10px;
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--tok-gutter);
}

/* ---- 顶部工具栏（替代原右下悬浮层，不遮挡代码区） ---- */
.je-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 24px;
  flex-shrink: 0;
}

.je-toolbar-spacer {
  flex: 1;
}

/* 校验状态 Tag：淡绿 / 淡红 / 中性灰，低调不抢焦点 */
.je-status {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 2px 9px;
  border-radius: 999px;
  font-size: 11px;
  line-height: 1.4;
  white-space: nowrap;
  color: #6ee7a0;
  background: rgba(52, 211, 153, 0.08);
}
.je-status.invalid {
  color: #f87171;
  background: rgba(239, 68, 68, 0.1);
}
.je-status.large {
  color: var(--text-3);
  background: var(--bg-hover);
}
.je-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
  flex-shrink: 0;
}

.je-actions {
  display: flex;
  align-items: center;
  gap: 4px;
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
  border-radius: 6px;
  background: transparent;
  color: var(--text-2);
  cursor: pointer;
  transition:
    background var(--dur) var(--ease),
    color var(--dur) var(--ease);
}
.je-btn:hover:not(:disabled) {
  background: var(--bg-hover);
  color: var(--text-1);
}
.je-btn:disabled {
  opacity: 0.35;
  cursor: default;
}

/* ---- 统一 JSON 语法着色（--tok-*，与响应 Body 共用，见 constants/editorTheme.ts） ---- */
:deep(.hl-k) {
  color: var(--tok-key);
  font-weight: 600;
}
:deep(.hl-s) {
  color: var(--tok-str);
}
:deep(.hl-n) {
  color: var(--tok-num);
}
:deep(.hl-b) {
  color: var(--tok-bool);
}
:deep(.hl-null) {
  color: var(--tok-null);
  font-style: italic;
}
:deep(.hl-p) {
  color: var(--tok-punct);
}
</style>
