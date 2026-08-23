<script setup lang="ts">
/**
 * ToolsDrawer：接口工具抽屉，点击请求栏「工具」按钮打开。
 * 承载低频工具：断言测试（fox-test 运行器）与压测（并发基准）。
 */
import Drawer from './ui/Drawer.vue'
import LoadTestPanel from './LoadTestPanel.vue'
import TestsPanel from './TestsPanel.vue'
import type { Endpoint } from '../types/foxApi'

defineProps<{
  open: boolean
  draft: Endpoint | null
  url: string
}>()

const emit = defineEmits<{ close: [] }>()
</script>

<template>
  <Drawer :open="open" title="接口工具（断言测试 / 压测）" width="560px" @close="emit('close')">
    <div class="tools">
      <section class="tool-sec">
        <h3 class="tool-title">断言测试</h3>
        <p class="tool-hint">配置状态码 / 响应头 / JSONPath 断言并运行，结果逐条展示。</p>
        <TestsPanel :draft="draft" :url="url" />
      </section>

      <section class="tool-sec">
        <h3 class="tool-title">压测（并发基准）</h3>
        <LoadTestPanel :draft="draft" :url="url" />
      </section>
    </div>
  </Drawer>
</template>

<style scoped>
.tools {
  display: flex;
  flex-direction: column;
  gap: 22px;
}

.tool-sec {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.tool-title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-1);
}

.tool-hint {
  margin: 0;
  font-size: 11.5px;
  color: var(--text-3);
}
</style>
