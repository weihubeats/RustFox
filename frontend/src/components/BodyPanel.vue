<script setup lang="ts">
/**
 * BodyPanel：请求体面板。
 * Tab Bar（Postman 风格）：none / form-data / x-www-form-urlencoded / raw / binary / graphql。
 * - raw 为 json+text 聚合视图：右侧子类型下拉（JSON/Text/JS/HTML/XML）切换
 *   编辑器并联动 Content-Type 请求头（映射逻辑见 utils/bodyMode.ts）；
 * - binary 为本地文件路径，发送时后端读取原始字节作为请求体；
 * - urlencoded/multipart 为字段行编辑，graphql 为 query/variables 编辑器。
 * bodyAny 用 any 放宽联合类型访问（模板 v-model 直写 raw / spec.*）。
 */
import { computed } from 'vue'
import CustomSelect from './ui/CustomSelect.vue'
import Icon from './ui/Icon.vue'
import IconButton from './ui/IconButton.vue'
import JsonEditor from './ui/JsonEditor.vue'
import KeyValueTable, { type KVRow } from './ui/KeyValueTable.vue'
import SegmentedControl, { type SegmentOption } from './ui/SegmentedControl.vue'
import { RAW_SUBTYPES, applyBodyTab, applyRawSubtype, rawSubtypeOf, tabOf } from '../utils/bodyMode'
import type { BodyTab, RawSubtype } from '../utils/bodyMode'
import type { BodySpec, Endpoint, GraphQLSpec, KeyValue, MultipartField } from '../types/foxApi'

const props = defineProps<{ draft: Endpoint | null }>()

/** 编辑视图：BodySpec 联合各分支字段统一为必填（运行时按 activeTab 分支存在）。 */
type EditableBody = {
  mode: string
  raw: string
  path: string
  spec: GraphQLSpec
  fields: KeyValue[] | MultipartField[]
}

const bodyAny = computed(() => props.draft?.request.body as EditableBody)
const headersAny = computed(() => props.draft?.request.headers as KeyValue[] | undefined)
const graphql = computed(() => bodyAny.value.spec)
const urlencodedFields = computed(() => bodyAny.value.fields as KeyValue[])
const multipartFields = computed(() => bodyAny.value.fields as MultipartField[])

const BODY_TABS: SegmentOption[] = [
  { value: 'none', label: '无' },
  { value: 'form-data', label: 'form-data' },
  { value: 'x-www-form-urlencoded', label: 'x-www-form-urlencoded' },
  { value: 'raw', label: 'raw' },
  { value: 'binary', label: 'binary' },
  { value: 'graphql', label: 'GraphQL' },
]

const RAW_SUBTYPE_OPTIONS = RAW_SUBTYPES.map((s) => ({ value: s.value, label: s.label }))

const activeTab = computed({
  get: () => tabOf((bodyAny.value ?? { mode: 'none' }) as BodySpec, headersAny.value ?? []),
  set: (tab: string) => {
    if (props.draft) applyBodyTab(props.draft.request, tab as BodyTab)
  },
})

const rawSubtype = computed({
  get: () => rawSubtypeOf((bodyAny.value ?? { mode: 'none' }) as BodySpec, headersAny.value ?? []),
  set: (subtype: string) => {
    if (props.draft) applyRawSubtype(props.draft.request, subtype as RawSubtype)
  },
})

const RAW_PLACEHOLDER: Record<RawSubtype, string> = {
  json: '{ "key": "value" }',
  text: '纯文本内容',
  javascript: '// JavaScript 代码',
  html: '<!DOCTYPE html>\n<html>…</html>',
  xml: '<?xml version="1.0" encoding="UTF-8"?>\n<root>…</root>',
}

const MULTIPART_TYPE_OPTIONS = [
  { value: 'text', label: '文本' },
  { value: 'file_path', label: '文件路径' },
]

/** urlencoded 字段表：与 Params/Headers 一致的 KeyValueTable 幽灵行（输入自动补行）。 */
function applyUrlencoded(rows: KVRow[]): void {
  const body = props.draft?.request.body as { fields: KeyValue[] } | undefined
  body?.fields.splice(0, body.fields.length, ...(rows as KeyValue[]))
}

function addMultipartField(): void {
  const fields = props.draft?.request.body as { fields: MultipartField[] } | undefined
  fields?.fields.push({ key: '', value_type: 'text', value: '', enabled: true })
}

function removeMultipartField(index: number): void {
  const fields = props.draft?.request.body as { fields: unknown[] } | undefined
  fields?.fields.splice(index, 1)
}
</script>

<template>
  <div class="panel">
    <div class="mode-bar">
      <SegmentedControl v-model="activeTab" :options="BODY_TABS" size="sm" class="mode-tabs" />
      <CustomSelect
        v-if="activeTab === 'raw'"
        v-model="rawSubtype"
        :options="RAW_SUBTYPE_OPTIONS"
        size="sm"
        class="raw-subtype"
        pop-class="raw-subtype-pop"
      />
    </div>

    <JsonEditor
      v-if="activeTab === 'raw' && rawSubtype === 'json'"
      v-model="bodyAny.raw"
      placeholder='{ "key": "value" }'
      :min-height="120"
    />
    <textarea
      v-else-if="activeTab === 'raw'"
      v-model="bodyAny.raw"
      class="rf-input body-input"
      spellcheck="false"
      :placeholder="RAW_PLACEHOLDER[rawSubtype as RawSubtype] ?? '纯文本内容'"
    ></textarea>

    <div v-else-if="activeTab === 'graphql'" class="gql-editor">
      <textarea
        v-model="graphql.query"
        class="rf-input body-input"
        spellcheck="false"
        placeholder="query Hero($id: ID!) { hero(id: $id) { name } }"
      ></textarea>
      <JsonEditor
        v-model="graphql.variables"
        placeholder='{ "id": "42" }'
        :min-height="80"
      />
      <input
        v-model="graphql.operation_name"
        class="rf-input rf-input-sm"
        placeholder="operationName（可选）"
      />
    </div>

    <KeyValueTable
      v-else-if="activeTab === 'x-www-form-urlencoded'"
      :model-value="urlencodedFields"
      @update:model-value="applyUrlencoded"
    />

    <div v-else-if="activeTab === 'form-data'" class="mp-table">
      <div class="mp-head">
        <span class="mp-col mp-check"></span>
        <span class="mp-col mp-key rf-mono">Key</span>
        <span class="mp-col mp-type rf-mono">类型</span>
        <span class="mp-col mp-value rf-mono">Value</span>
        <span class="mp-col mp-actions"></span>
      </div>
      <div
        v-for="(f, i) in multipartFields"
        :key="i"
        class="mp-row"
        :class="{ off: f.enabled === false }"
      >
        <span class="mp-col mp-check">
          <input v-model="f.enabled" type="checkbox" class="mp-check-box" />
        </span>
        <input v-model="f.key" class="mp-input mp-col mp-key" placeholder="Key" spellcheck="false" />
        <CustomSelect
          v-model="f.value_type"
          :options="MULTIPART_TYPE_OPTIONS"
          size="sm"
          class="mp-type-select"
        />
        <input
          v-model="f.value"
          class="mp-input mp-col mp-value"
          :placeholder="f.value_type === 'file_path' ? '/path/to/file' : 'Value'"
          spellcheck="false"
          @keydown.enter.prevent="addMultipartField"
        />
        <span class="mp-col mp-actions">
          <IconButton name="trash" :size="13" tone="danger" title="删除" @click="removeMultipartField(i)" />
        </span>
      </div>
      <button class="mp-add" type="button" @click="addMultipartField">
        <Icon name="plus" :size="13" /> 添加字段
      </button>
    </div>

    <div v-else-if="activeTab === 'binary'" class="binary-box">
      <label class="binary-label">
        <Icon name="upload" :size="14" /> 文件路径
      </label>
      <input
        v-model="bodyAny.path"
        class="rf-input rf-input-sm binary-input"
        spellcheck="false"
        placeholder="/path/to/file.bin（如 /Users/me/avatar.png）"
      />
      <p class="binary-hint">
        发送时后端读取该文件的原始字节作为请求体；Content-Type 默认
        application/octet-stream，可在 Headers 标签改为实际类型（如 image/png）。
      </p>
    </div>

    <p v-else class="body-hint">该请求不携带 Body。</p>
  </div>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
  flex: 1;
  min-height: 0;
}

/* 主编辑器（JsonEditor 直接子级）：底部无边框，与分割条无缝贴合 */
.panel > :deep(.json-editor .hl-wrap) {
  border-bottom: none;
  border-bottom-left-radius: 0;
  border-bottom-right-radius: 0;
}

.mode-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.mode-tabs {
  flex: 0 1 auto;
  min-width: 0;
}

.raw-subtype {
  width: 130px;
  flex-shrink: 0;
}

.body-input {
  width: 100%;
  min-height: 120px;
  flex: 1;
  font-family: var(--font-mono);
  font-size: 12.5px;
  resize: vertical;
  border-bottom: none;
  border-bottom-left-radius: 0;
  border-bottom-right-radius: 0;
}

.gql-editor {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 1;
  min-height: 0;
}

/* ---- form-data 字段表：与 KeyValueTable 同视觉语言（表头 + 行分隔 + 悬停高亮） ---- */
.mp-table {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  overflow: hidden;
  background: var(--bg-card);
}

.mp-head,
.mp-row {
  display: flex;
  align-items: center;
}

.mp-head {
  height: 28px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-panel);
  font-size: 10.5px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-3);
}

.mp-row {
  min-height: 32px;
  border-bottom: 1px solid var(--border);
  transition: background var(--dur) var(--ease);
}
.mp-row:last-of-type {
  border-bottom: none;
}
.mp-row:hover {
  background: var(--bg-hover);
}
.mp-row.off .mp-input {
  opacity: 0.45;
}

.mp-col {
  flex-shrink: 0;
}
.mp-check {
  width: 36px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.mp-key {
  width: 34%;
  min-width: 120px;
}
.mp-type {
  width: 110px;
}
.mp-value {
  flex: 1;
  min-width: 0;
}
.mp-actions {
  width: 32px;
  display: inline-flex;
  align-items: center;
  justify-content: flex-end;
  opacity: 0;
  transition: opacity var(--dur) var(--ease);
}
.mp-row:hover .mp-actions,
.mp-row:focus-within .mp-actions {
  opacity: 1;
}

.mp-check-box {
  accent-color: var(--accent);
  cursor: pointer;
}

.mp-type-select {
  width: 96px;
  flex-shrink: 0;
  margin-right: 6px;
}

.mp-input {
  height: 32px;
  border: none;
  background: transparent;
  color: var(--text-1);
  font-size: 12px;
  outline: none;
  padding: 0 8px;
  min-width: 0;
}
.mp-input::placeholder {
  color: var(--text-3);
}
.mp-input:focus {
  background: var(--bg-elevated);
  box-shadow: inset 0 0 0 1px var(--accent);
}

/* 添加行：轻量虚线整行按钮（hover 提亮为主色），替代笨重的实心通栏按钮 */
.mp-add {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 32px;
  border: none;
  border-top: 1px dashed var(--border);
  background: transparent;
  color: var(--text-3);
  font-size: 12px;
  font-family: inherit;
  cursor: pointer;
  transition: color var(--dur) var(--ease), background var(--dur) var(--ease);
}
.mp-add:hover {
  color: var(--accent);
  background: var(--accent-tint);
}

.binary-box {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 1;
  min-height: 0;
}

.binary-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-2);
}

.binary-label svg {
  color: var(--accent);
}

.binary-input {
  width: 100%;
  font-family: var(--font-mono);
}

.binary-hint {
  margin: 0;
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--text-3);
}

.body-hint {
  margin: 0;
  font-size: 12px;
  color: var(--text-3);
}
</style>
