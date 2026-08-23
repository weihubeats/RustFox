<script setup lang="ts">
/**
 * Modal：居中模态弹层。
 * - v-model:open 受控；Esc / 遮罩点击 / 右上 ✕ 关闭（可经 guardClose 拦截）；
 * - 打开后自动聚焦首个可聚焦元素（autofocus 默认开启），关闭后焦点归还触发元素；
 * - Tab 焦点陷阱：焦点始终圈定在对话框内，不穿透遮罩到背景内容；
 * - 打开期间锁定 body 滚动；Teleport 到 body，双主题。
 */
import { nextTick, ref, watch } from 'vue'
import IconButton from './IconButton.vue'

const props = withDefaults(
  defineProps<{
    open: boolean
    title?: string
    width?: string
    closable?: boolean
    autofocus?: boolean
    /** 关闭前守卫：返回 false 拦截 Esc / 遮罩 / ✕ 触发的关闭（用于脏数据确认）。 */
    guardClose?: () => boolean
  }>(),
  { title: '', width: '420px', closable: true, autofocus: true, guardClose: undefined },
)

const emit = defineEmits<{
  'update:open': [open: boolean]
  close: []
}>()

const titleId = `rf-modal-title-${Math.random().toString(36).slice(2, 9)}`

const dialogEl = ref<HTMLElement | null>(null)

/** 关闭请求统一入口：守卫放行才真正关闭。 */
function requestClose(): void {
  if (props.guardClose && !props.guardClose()) return
  emit('update:open', false)
  emit('close')
}

/** 对话框内可聚焦元素（Tab 陷阱的目标集合）。 */
function focusables(): HTMLElement[] {
  if (!dialogEl.value) return []
  return Array.from(
    dialogEl.value.querySelectorAll<HTMLElement>(
      'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ),
  )
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.preventDefault()
    requestClose()
    return
  }
  if (event.key !== 'Tab') return
  const root = dialogEl.value
  if (!root) return
  const items = focusables()
  if (!items.length) {
    event.preventDefault()
    root.focus()
    return
  }
  const first = items[0]
  const last = items[items.length - 1]
  const active = document.activeElement
  const inside = !!active && root.contains(active)
  if (event.shiftKey) {
    // 逆序走到头（或焦点在对话框外）→ 回到最后一个
    if (!inside || active === first) {
      event.preventDefault()
      last.focus()
    }
  } else if (!inside || active === last) {
    event.preventDefault()
    first.focus()
  }
}

/** 关闭时把焦点还给触发元素，避免屏幕阅读器/键盘用户丢失位置。 */
let lastFocused: HTMLElement | null = null

watch(
  () => props.open,
  (open) => {
    if (open) {
      lastFocused = document.activeElement as HTMLElement | null
      document.body.style.overflow = 'hidden'
      document.addEventListener('keydown', onKeydown)
      if (props.autofocus) {
        nextTick(() => {
          focusables()[0]?.focus()
        })
      }
    } else {
      document.body.style.overflow = ''
      document.removeEventListener('keydown', onKeydown)
      lastFocused?.focus?.()
      lastFocused = null
    }
  },
)
</script>

<template>
  <Teleport to="body">
    <Transition name="m">
      <div v-if="open" class="m-mask" @mousedown.self="closable && requestClose()">
        <div
          ref="dialogEl"
          class="m-dialog"
          role="dialog"
          aria-modal="true"
          :aria-labelledby="title ? titleId : undefined"
          tabindex="-1"
          :style="{ width }"
          @mousedown.stop
        >
          <div v-if="title" class="m-head">
            <h3 :id="titleId" class="m-title">{{ title }}</h3>
            <IconButton v-if="closable" name="x" :size="14" title="关闭" @click="requestClose" />
          </div>
          <div class="m-body">
            <slot />
          </div>
          <div v-if="$slots.footer" class="m-foot">
            <slot name="footer" />
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.m-mask {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: var(--mask);
}

.m-dialog {
  display: flex;
  flex-direction: column;
  max-width: 92vw;
  max-height: 84vh;
  border-radius: var(--radius-lg);
  border: 1px solid var(--border-strong);
  background: var(--bg-elevated);
  box-shadow: var(--shadow-lg);
  overflow: hidden;
  outline: none;
}

.m-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 14px 16px 0;
}

.m-title {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  color: var(--text-1);
}

.m-body {
  padding: 12px 16px 16px;
  overflow-y: auto;
  color: var(--text-2);
  font-size: 13px;
}

.m-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 0 16px 14px;
}

.m-enter-active,
.m-leave-active {
  transition: opacity 160ms var(--ease);
}
.m-enter-active .m-dialog,
.m-leave-active .m-dialog {
  transition: transform 160ms var(--ease);
}
.m-enter-from,
.m-leave-to {
  opacity: 0;
}
.m-enter-from .m-dialog,
.m-leave-to .m-dialog {
  transform: translateY(-10px) scale(0.98);
}
</style>