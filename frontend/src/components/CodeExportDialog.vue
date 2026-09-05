<script setup lang="ts">
/**
 * CodeExportDialog：接口代码导出弹窗。
 * 复用 CodePanel（语言选择 → 生成 → 复制），打开即按当前请求配置自动生成
 * curl / Python / JavaScript / Go / Java / PHP 代码片段。
 */
import CodePanel from './CodePanel.vue'
import Modal from './ui/Modal.vue'
import { useLocaleStore } from '../stores/locale'
import type { Endpoint } from '../types/foxApi'

defineProps<{ draft: Endpoint | null; url: string }>()
const emit = defineEmits<{ close: [] }>()

const locale = useLocaleStore()
const t = locale.t
</script>

<template>
  <Modal :open="true" :title="t('codeexport.title')" width="680px" @close="emit('close')">
    <p class="modal-hint">
      {{ t('codeexport.hint') }}
    </p>
    <CodePanel :draft="draft" :url="url" auto-generate />
    <template #footer>
      <button class="rf-btn rf-btn-primary" type="button" @click="emit('close')">{{ t('common.close') }}</button>
    </template>
  </Modal>
</template>

<style scoped>
.modal-hint {
  margin: 0 0 10px;
  font-size: 12.5px;
  color: var(--text-2);
}
</style>