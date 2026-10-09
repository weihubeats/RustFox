<script setup lang="ts">
/**
 * ReleaseNotesModal：独立版本更新详情弹窗（顶部更新 Tag / 设置更新卡片的统一落地页）。
 * - Header：🚀 发现新版本 vX（白色加粗等宽，样式在文末非 scoped 段覆写 Modal 容器/标题）；
 * - Body：Markdown 更新日志（markdownLite 转义渲染，深底包裹框固定高度可滚动，无序列表紫点）；
 * - Footer：稍后再说（次要按钮）+ 立即下载并重启（主紫按钮），下载中就地展示进度条；
 * - 打开时接管共享暂存的 Update；未安装关闭（Esc / 遮罩 / ✕ / 稍后再说）放回暂存，
 *   顶部 Tag 复亮；下载/安装中禁止关闭，成功后自动关闭并延迟重启。
 */
import { computed, ref, watch } from 'vue'
import type { Update } from '@tauri-apps/plugin-updater'
import { restorePendingUpdate, takePendingUpdate } from '../composables/useAutoUpdate'
import { useUpdateDownload } from '../composables/useUpdateDownload'
import { useLocaleStore } from '../stores/locale'
import { renderMarkdownLite } from '../utils/markdownLite'
import Modal from './ui/Modal.vue'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ 'update:open': [open: boolean] }>()

const t = useLocaleStore().t

const { downloading, progress, installing, failMsg, statusText, start } = useUpdateDownload()

/** 待安装版本号与更新说明（打开时从共享暂存接管）。 */
const version = ref('')
const notes = ref('')
/** 本地接管的 Update 对象（关闭未安装时放回共享暂存）。 */
let held: Update | null = null

const title = computed(() => `🚀 ${t('app.updateFound', { v: version.value })}`)
const notesHtml = computed(() => renderMarkdownLite(notes.value))

watch(
  () => props.open,
  (open) => {
    if (!open) {
      if (held) {
        // 下载中不可达（guardClose 拦截）；未安装关闭 → 放回暂存，小红点复亮
        if (downloading.value) return
        restorePendingUpdate(held)
        held = null
      }
      version.value = ''
      notes.value = ''
      return
    }
    if (held) return
    const update = takePendingUpdate()
    if (update?.available) {
      held = update
      version.value = update.version
      notes.value = update.body ?? ''
    } else {
      // 暂存已被别处（关于弹窗）取走：无可展示内容，立即自关
      update?.close()
      emit('update:open', false)
    }
  },
)

/** 立即下载并重启：成功后清持有并自动关闭（状态机负责 toast + 延迟重启）。 */
async function install(): Promise<void> {
  if (!held || downloading.value) return
  const ok = await start(held)
  if (ok) {
    held = null
    emit('update:open', false)
  }
}

/** 稍后再说：关闭弹窗（watch 放回暂存，更新提醒保留）。 */
function later(): void {
  if (downloading.value) return
  emit('update:open', false)
}

/** Esc / 遮罩 / ✕ 关闭守卫：下载与安装中禁止关闭（进度就地可见）。 */
function guardClose(): boolean {
  return !downloading.value
}
</script>

<template>
  <Modal
    :open="open"
    :title="title"
    width="440px"
    dialog-class="rn-modal"
    mask-class="backdrop-blur-sm rn-mask"
    :guard-close="guardClose"
    @update:open="emit('update:open', $event)"
  >
    <!-- 固定高度 + 滚动；深底浅字在深/浅主题下均可读 -->
    <div
      class="my-4 min-h-[80px] max-h-[200px] overflow-y-auto rounded-xl border border-white/5 bg-zinc-950/80 p-4 text-xs leading-relaxed text-zinc-300"
    >
      <div v-if="notesHtml" class="rn-notes" v-html="notesHtml"></div>
      <p v-else class="m-0 text-xs text-zinc-400">{{ t('release.noNotes') }}</p>
    </div>

    <template #footer>
      <div class="rn-foot">
        <div v-if="downloading" class="rn-progress-wrap">
          <div
            class="rn-progress"
            role="progressbar"
            aria-valuemin="0"
            aria-valuemax="100"
            :aria-valuenow="progress == null ? undefined : Math.round(progress * 100)"
            :aria-valuetext="statusText"
          >
            <div
              class="rn-progress-bar"
              :class="{ indeterminate: progress == null && !installing }"
              :style="{ width: progress == null ? '100%' : `${Math.round(progress * 100)}%` }"
            ></div>
          </div>
          <span class="rn-status">{{ statusText }}</span>
        </div>
        <p v-else-if="failMsg" class="rn-error" role="alert">{{ failMsg }}</p>
        <div class="rn-actions">
          <button
            class="cursor-pointer rounded-lg px-4 py-2 text-xs font-medium text-zinc-400 transition-all hover:bg-white/5 hover:text-zinc-200 disabled:cursor-default disabled:opacity-50"
            type="button"
            :disabled="downloading"
            @click="later"
          >
            {{ t('release.later') }}
          </button>
          <button
            class="rounded-lg bg-purple-600 px-4 py-2 text-xs font-medium text-white shadow-lg shadow-purple-600/30 transition-all hover:bg-purple-500 disabled:cursor-default disabled:opacity-60"
            type="button"
            :disabled="downloading"
            @click="install"
          >
            {{ t('release.install') }}
          </button>
        </div>
      </div>
    </template>
  </Modal>
</template>

<style scoped>
/* 段落/列表纵向行距（等价 space-y-1.5：非首子元素 6px 上边距） */
.rn-notes > :deep(* + *) {
  margin-top: 6px;
}

/* 无序列表：半透明紫点 marker（字号/行高继承包裹框 text-xs） */
.rn-notes :deep(ul > li::marker) {
  color: rgba(196, 181, 253, 0.55);
}

/* 深底容器内的排版（浅字固定色，深/浅主题均落在深底上） */
.rn-notes :deep(h3),
.rn-notes :deep(h4),
.rn-notes :deep(h5),
.rn-notes :deep(h6) {
  margin: 10px 0 4px;
  color: #fafafa;
  font-size: 13px;
  font-weight: 600;
}
.rn-notes :deep(h3:first-child),
.rn-notes :deep(h4:first-child),
.rn-notes :deep(h5:first-child),
.rn-notes :deep(h6:first-child) {
  margin-top: 0;
}
.rn-notes :deep(p) {
  margin: 0 0 6px;
}
.rn-notes :deep(p:last-child) {
  margin-bottom: 0;
}
.rn-notes :deep(ul),
.rn-notes :deep(ol) {
  margin: 0 0 6px;
  padding-left: 18px;
}
.rn-notes :deep(li) {
  margin: 2px 0;
}
.rn-notes :deep(strong) {
  color: #ffffff;
}
.rn-notes :deep(code) {
  padding: 1px 5px;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.08);
  font-family: var(--font-mono);
  font-size: 11.5px;
}
.rn-notes :deep(pre) {
  margin: 0 0 6px;
  padding: 8px;
  overflow-x: auto;
  border-radius: 6px;
  background: rgba(0, 0, 0, 0.4);
}
.rn-notes :deep(pre code) {
  padding: 0;
  background: none;
}
.rn-notes :deep(a) {
  color: #c4b5fd;
  text-decoration: underline;
}
.rn-notes :deep(hr) {
  margin: 8px 0;
  border: none;
  border-top: 1px solid rgba(255, 255, 255, 0.12);
}
.rn-notes :deep(blockquote) {
  margin: 0 0 6px;
  padding-left: 8px;
  border-left: 2px solid rgba(255, 255, 255, 0.2);
  color: #a1a1aa;
}

/* Footer：进度 / 错误 + 按钮行 */
.rn-foot {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
}
.rn-progress-wrap {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
}
.rn-progress {
  width: 100%;
  height: 6px;
  overflow: hidden;
  border-radius: 999px;
  background: var(--bg-3, rgba(127, 127, 127, 0.2));
}
.rn-progress-bar {
  height: 100%;
  border-radius: 999px;
  background: var(--accent);
  transition: width 0.2s ease;
}
.rn-progress-bar.indeterminate {
  animation: rn-slide 1.2s ease-in-out infinite;
}
@keyframes rn-slide {
  0% {
    transform: translateX(-100%);
  }
  100% {
    transform: translateX(100%);
  }
}
.rn-status {
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--text-2);
}
.rn-error {
  margin: 0;
  font-size: 12px;
  line-height: 1.45;
  color: var(--danger, #f85149);
  word-break: break-all;
}
.rn-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>

<style>
/* Modal 遮罩/容器经 Teleport 挂到 body，覆写须非 scoped（同 SettingsDialog .sd-dialog 的变量先例）；
   类名 rn-modal / rn-mask 仅本弹窗使用，不会波及其他 Modal。
   选择器加 body 前缀保证特异性压过 Modal 自带的 scoped 规则（.m-xxx[data-v]）。 */

/* 遮罩：bg-black/60 走 --mask 变量（元素级声明直接生效），高斯模糊走 backdrop-blur-sm 类 */
.rn-mask {
  --mask: rgba(0, 0, 0, 0.6);
}

/* 容器：rounded-2xl / border-white/10 / bg-zinc-900/95 / shadow-2xl，经变量覆写；双主题恒深底，
   文本令牌一并覆写，浅色主题下正文不发黑 */
.rn-modal {
  --bg-elevated: rgba(24, 24, 27, 0.95);
  --border-strong: rgba(255, 255, 255, 0.1);
  --radius-lg: 16px;
  --shadow-lg: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
  --text-1: #fafafa;
  --text-2: #a1a1aa;
  --text-3: #71717a;
  --bg-3: rgba(255, 255, 255, 0.14);
}

/* p-6：内边距统一收到容器，内部三段归零 */
body .rn-modal.m-dialog {
  padding: 24px;
}
/* Header：flex 垂直居中 + 底分隔线（justify-between/space-between 由 Modal .m-head 提供） */
body .rn-modal .m-head {
  padding: 0 0 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.05);
}
body .rn-modal .m-body {
  padding: 0;
}
body .rn-modal .m-foot {
  padding: 0;
}

/* 标题：白色加粗等宽，与右侧关闭 Icon 垂直对齐（m-head 已 flex center） */
body .rn-modal .m-title {
  font-family: var(--font-mono);
  font-size: 14px;
  font-weight: 700;
  color: #ffffff;
}
</style>
