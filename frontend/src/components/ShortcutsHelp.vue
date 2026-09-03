<script setup lang="ts">
/**
 * ShortcutsHelp：快捷键帮助面板。
 * 内容由全局注册表（useShortcuts）生成，新增快捷键自动收录；
 * 输入框内局部快捷键（响应查找等）作为静态补充列出。
 */
import { computed } from 'vue'
import Modal from './ui/Modal.vue'
import { shortcutGroups, shortcutLabel } from '../composables/useShortcuts'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ 'update:open': [open: boolean] }>()

const groups = computed(() => shortcutGroups())

/** 输入框局部快捷键（非全局注册，此处静态说明）。 */
const contextual = [
  { keys: '⌘/Ctrl + F', description: '在响应中查找' },
  { keys: 'Enter', description: '地址栏回车发送请求' },
  { keys: 'Esc', description: '清空地址栏路径 / 关闭弹层' },
]
</script>

<template>
  <Modal
    :open="props.open"
    title="快捷键"
    width="480px"
    @update:open="emit('update:open', $event)"
    @close="emit('update:open', false)"
  >
    <div v-for="g in groups" :key="g.group" class="sc-group">
      <p class="sc-group-title">{{ g.group }}</p>
      <div v-for="item in g.items" :key="item.id" class="sc-row">
        <span class="sc-desc">{{ item.description }}</span>
        <kbd class="sc-keys">{{ shortcutLabel(item) }}</kbd>
      </div>
    </div>
    <div class="sc-group">
      <p class="sc-group-title">输入框内</p>
      <div v-for="c in contextual" :key="c.keys + c.description" class="sc-row">
        <span class="sc-desc">{{ c.description }}</span>
        <kbd class="sc-keys">{{ c.keys }}</kbd>
      </div>
    </div>
    <p v-if="!groups.length" class="sc-empty">当前视图无全局快捷键</p>
  </Modal>
</template>

<style scoped>
.sc-group {
  margin-bottom: 14px;
}
.sc-group-title {
  margin: 0 0 6px;
  font-size: 11.5px;
  font-weight: 700;
  color: var(--text-3);
  text-transform: uppercase;
  letter-spacing: 0.4px;
}
.sc-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 4px 0;
  font-size: 12.5px;
}
.sc-desc {
  color: var(--text-1);
}
.sc-keys {
  flex-shrink: 0;
  font-family: var(--font-mono);
  font-size: 11px;
  padding: 2px 8px;
  border: 1px solid var(--border);
  border-bottom-width: 2px;
  border-radius: 6px;
  background: var(--bg-hover);
  color: var(--text-2);
  white-space: nowrap;
}
.sc-empty {
  font-size: 12px;
  color: var(--text-3);
}
</style>
