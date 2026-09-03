<script setup lang="ts">
/**
 * HeadersPanel：请求头面板（Postman 式 kv 表格：幽灵行自动追加、空行自动清理）。
 */
import { computed } from 'vue'
import KeyValueTable, { type KVRow } from './ui/KeyValueTable.vue'
import type { Endpoint, KeyValue } from '../types/foxApi'

const props = defineProps<{ draft: Endpoint | null }>()

const headers = computed(() => props.draft?.request.headers ?? [])

/** 单请求 Cookie 回放开关（默认开启；关闭走无 jar 客户端）。 */
const cookiesEnabled = computed(() => !(props.draft?.request.disable_cookies ?? false))

function onCookiesToggle(enabled: boolean): void {
  const request = props.draft?.request
  if (request) request.disable_cookies = !enabled
}

function applyHeaders(rows: KVRow[]): void {
  headers.value.splice(0, headers.value.length, ...(rows as KeyValue[]))
}
</script>

<template>
  <div class="panel">
    <label
      class="cookie-toggle"
      title="关闭后本次请求不携带 Jar 中的同域 Cookie（显式写的 Cookie 头不受影响）"
    >
      <input
        :checked="cookiesEnabled"
        type="checkbox"
        @change="onCookiesToggle(($event.target as HTMLInputElement).checked)"
      />
      Cookie 自动回放
    </label>
    <KeyValueTable
      :model-value="headers"
      key-placeholder="Header"
      value-placeholder="Value"
      description-placeholder="描述"
      @update:model-value="applyHeaders"
    />
  </div>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.cookie-toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-2);
  cursor: pointer;
  user-select: none;
}
</style>