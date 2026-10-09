<script setup lang="ts">
/**
 * DeleteConfirmModal：删除确认居中弹窗（接口 / 文件夹删除统一落地）。
 * - 柔和模糊遮罩（backdrop-blur-sm bg-black/50）+ 深底卡片容器（w-400 rounded-2xl p-6，
 *   双主题恒深色，经 CSS 变量覆写，同 ReleaseNotesModal 先例）；
 * - 布局：警告徽章 + 基础标题（「删除接口」）+ 句式提示（含名称与不可撤销）+
 *   method/path 语义化代码卡片（方法色走 methodTone 单一真源）；
 * - 取消（Ghost）+ 确认删除（红实心 + 深红发光）；Esc / 遮罩 / ✕ 可关。
 */
import { useLocaleStore } from '../stores/locale'
import { methodTone } from '../utils/methodTone'
import Icon from './ui/Icon.vue'
import Modal from './ui/Modal.vue'

defineProps<{
  open: boolean
  /** 基础标题（如「删除接口」/「删除文件夹」）。 */
  title: string
  /** 句式提示（含被删对象名称与不可撤销说明），由调用方经 i18n 组装。 */
  message: string
  /** HTTP 方法（接口删除时传入，渲染语义化徽章；空则不渲染代码卡片）。 */
  method?: string
  /** 请求路径（与 method 同现时渲染代码卡片）。 */
  path?: string
  /** 确认按钮文案（默认「确认删除」）。 */
  confirmLabel?: string
}>()
const emit = defineEmits<{ 'update:open': [open: boolean]; confirm: [] }>()

const t = useLocaleStore().t
</script>

<template>
  <Modal
    :open="open"
    width="400px"
    dialog-class="dc-modal"
    mask-class="backdrop-blur-sm dc-mask"
    @update:open="emit('update:open', $event)"
  >
    <!-- 右上快捷关闭（容器 position: relative 定位，见文末样式） -->
    <button
      class="dc-close"
      type="button"
      :aria-label="t('common.close')"
      @click="emit('update:open', false)"
    >
      <Icon name="x" :size="14" />
    </button>
    <div class="dc-head">
      <span class="dc-badge"><Icon name="alert-triangle" :size="16" /></span>
      <div class="dc-texts">
        <p class="dc-title">{{ title }}</p>
        <p class="dc-message">{{ message }}</p>
        <!-- method/path 代码卡片：方法徽章走 methodTone（@theme 方法色单源），路径等宽高亮 -->
        <div v-if="method && path" class="dc-code">
          <span class="dc-method" :class="methodTone(method)">{{ method }}</span>
          <span class="dc-path">{{ path }}</span>
        </div>
      </div>
    </div>

    <template #footer>
      <div class="dc-actions">
        <button class="dc-cancel" type="button" @click="emit('update:open', false)">
          {{ t('common.cancel') }}
        </button>
        <button class="dc-confirm" type="button" @click="emit('confirm')">
          {{ confirmLabel || t('confirm.delete') }}
        </button>
      </div>
    </template>
  </Modal>
</template>

<style scoped>
.dc-head {
  display: flex;
  align-items: flex-start;
  gap: 16px;
}

/* 警告徽章：圆角红底 red-500/15 + red-400 图标 */
.dc-badge {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 8px;
  border-radius: 12px;
  background: rgba(239, 68, 68, 0.15);
  color: #f87171;
}

.dc-texts {
  min-width: 0;
  /* 让位右上角关闭图标 */
  padding-right: 16px;
}

.dc-title {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
  color: #ffffff;
  word-break: break-word;
}

.dc-message {
  margin: 4px 0 0;
  font-size: 12px;
  line-height: 1.55;
  color: #a1a1aa;
  word-break: break-word;
}

/* method/path 代码卡片：微暗容器 + 方法徽章 + 等宽路径 */
.dc-code {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
  padding: 10px;
  border: 1px solid rgba(255, 255, 255, 0.05);
  border-radius: 12px;
  background: rgba(9, 9, 11, 0.6);
}

.dc-method {
  flex-shrink: 0;
  padding: 2px 8px;
  border-width: 1px;
  border-style: solid; /* 描边色由 methodTone 的 border-method-*／20 类提供，此处不设色避免压过工具类 */
  border-radius: 6px;
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 600;
}

.dc-path {
  min-width: 0;
  overflow: hidden;
  font-family: var(--font-mono);
  font-size: 12px;
  color: #d4d4d8;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dc-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  width: 100%;
}

/* 右上快捷关闭：zinc-500 → hover 白 */
.dc-close {
  position: absolute;
  top: 16px;
  right: 16px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 2px;
  border: none;
  background: none;
  color: #71717a;
  cursor: pointer;
  transition: color var(--dur) var(--ease);
}
.dc-close:hover {
  color: #ffffff;
}

.dc-cancel {
  padding: 6px 14px;
  border: none;
  border-radius: 8px;
  background: none;
  font-family: inherit;
  font-size: 12px;
  font-weight: 500;
  color: #a1a1aa;
  cursor: pointer;
  transition: all var(--dur) var(--ease);
}
.dc-cancel:hover {
  background: rgba(255, 255, 255, 0.05);
  color: #e4e4e7;
}

/* 高危按钮：红底 + 红描边 + 深红发光（shadow-red-950/60），减弱纯红刺眼感 */
.dc-confirm {
  padding: 6px 16px;
  border: 1px solid rgba(239, 68, 68, 0.3);
  border-radius: 8px;
  background: #dc2626;
  font-family: inherit;
  font-size: 12px;
  font-weight: 500;
  color: #fff;
  box-shadow:
    0 10px 15px -3px rgba(127, 29, 29, 0.6),
    0 4px 6px -4px rgba(127, 29, 29, 0.6);
  cursor: pointer;
  transition: all var(--dur) var(--ease);
}
.dc-confirm:hover {
  background: #ef4444;
}
</style>

<style>
/* 遮罩/容器经 Teleport 挂 body，覆写须非 scoped；body 前缀压过 Modal 的 scoped 规则。
   类名 dc-modal / dc-mask 仅本弹窗使用。 */

/* 遮罩 bg-black/50 走 --mask 变量，高斯模糊走 backdrop-blur-sm 类 */
.dc-mask {
  --mask: rgba(0, 0, 0, 0.5);
}

/* 容器：w-[400px]（width prop）rounded-2xl bg-zinc-900/95 border-white/10 shadow-2xl
   + backdrop-blur-md；双主题恒深底，文本令牌一并覆写，浅色主题下不发黑 */
.dc-modal {
  --bg-elevated: rgba(24, 24, 27, 0.95);
  --border-strong: rgba(255, 255, 255, 0.1);
  --radius-lg: 16px;
  --shadow-lg: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
  --text-1: #fafafa;
  --text-2: #a1a1aa;
  --text-3: #71717a;
  /* 右上关闭图标的定位锚点 */
  position: relative;
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
}

/* p-6：内边距统一收到容器，内部两段归零 */
body .dc-modal.m-dialog {
  padding: 24px;
}
body .dc-modal .m-body {
  padding: 0;
}
body .dc-modal .m-foot {
  padding: 0;
  margin-top: 16px;
}
</style>
