<script setup lang="ts">
/**
 * HeaderUpdateTag：顶部导航更新入口（设置齿轮左侧）。
 * 检出新版本时常驻轻量胶囊 Tag（微亮火箭图标 + 更新 vX），点击打开独立
 * 更新详情弹窗 ReleaseNotesModal；安装 / 跳过 / 取走暂存后自动消失。
 */
import {
  hasPendingUpdate,
  pendingUpdateVersion,
  requestOpenRelease,
} from '../composables/useAutoUpdate'
import { useLocaleStore } from '../stores/locale'

const t = useLocaleStore().t
</script>

<template>
  <button
    v-if="hasPendingUpdate"
    class="flex cursor-pointer items-center gap-1.5 rounded-full border border-purple-500/40 bg-purple-500/20 px-3 py-1 font-mono text-xs font-medium text-purple-700 shadow-sm shadow-purple-950/50 transition-all hover:bg-purple-500/30 dark:text-purple-300"
    type="button"
    :title="t('app.updateFound', { v: pendingUpdateVersion() ?? '' })"
    @click="requestOpenRelease()"
  >
    <span class="rf-update-tag-icon" aria-hidden="true">🚀</span>
    <span>{{ t('app.updateTag', { v: pendingUpdateVersion() ?? '' }) }}</span>
  </button>
</template>

<style scoped>
/* 火箭微亮呼吸：prefers-reduced-motion 由 style.css 全局兜底关闭 */
.rf-update-tag-icon {
  font-size: 11px;
  line-height: 1;
  animation: rf-update-tag-glow 1.8s ease-in-out infinite;
}

@keyframes rf-update-tag-glow {
  0%,
  100% {
    transform: scale(1);
    opacity: 1;
  }
  50% {
    transform: scale(1.2);
    opacity: 0.7;
  }
}
</style>
