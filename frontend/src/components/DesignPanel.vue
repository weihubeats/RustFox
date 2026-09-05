<script setup lang="ts">
/**
 * DesignPanel：接口设计（ApiDesign）——API 契约的可视化定义。
 *
 * - 顶部固定操作栏：右侧「保存设计」主按钮 + 未保存黄点提醒（保存逻辑由
 *   EndpointEditor 提供：未命名接口先弹名称/位置确认框）；
 * - 基本信息卡片：名称 / 状态双列网格，Method + Path 组合输入组，描述；
 * - 请求定义：Params / Headers / Body 三 Tab，参数表支持
 *   参数名 | 类型 | 必填 | 说明 | 示例值（KeyValue 设计元数据随 request_json 持久化）；
 * - 返回响应 (Responses)：按状态码维护响应示例（复用 response_examples 存储）；
 * - 右侧实时预览栏：Schema 结构 / Mock 示例双视图，随草稿修改即时刷新。
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useWorkspaceStore } from '../stores/workspace'
import { useFoxApi } from '../composables/useFoxApi'
import { useToast } from '../composables/useToast'
import ParamDefineTable from './design/ParamDefineTable.vue'
import CustomSelect from './ui/CustomSelect.vue'
import Icon from './ui/Icon.vue'
import IconButton from './ui/IconButton.vue'
import Popconfirm from './ui/Popconfirm.vue'
import SegmentedControl from './ui/SegmentedControl.vue'
import Tabs from './ui/Tabs.vue'
import type { TabItem } from './ui/Tabs.vue'
import Tooltip from './ui/Tooltip.vue'
import { highlightJSON } from '../utils/highlight'
import { statusTextOf } from '../utils/testCases'
import { inferSchema, mockJsonFromSchema } from '../utils/schemaInfer'
import type { SchemaRow } from '../utils/schemaInfer'
import type {
  Endpoint,
  EndpointStatus,
  HttpMethod,
  KeyValue,
  MultipartField,
  ResponseExample,
} from '../types/foxApi'

const props = defineProps<{ draft: Endpoint | null }>()

const emit = defineEmits<{ save: [] }>()

const store = useWorkspaceStore()
const api = useFoxApi()
const toast = useToast()

const d = computed(() => props.draft)

/** 草稿是否含未保存修改（顶部黄点）。 */
const dirty = computed(() => (d.value ? store.isDirty(d.value.id) : false))

const METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS']
const METHOD_OPTIONS = METHODS.map((m) => ({ value: m, label: m }))

const STATUS_OPTIONS: { value: EndpointStatus; label: string }[] = [
  { value: 'designing', label: '设计中' },
  { value: 'developing', label: '开发中' },
  { value: 'testing', label: '测试中' },
  { value: 'released', label: '已发布' },
  { value: 'deprecated', label: '已废弃' },
]

function onMethodChange(v: string | number): void {
  const target = d.value
  if (target) target.method = String(v) as HttpMethod
}

function onStatusChange(v: string | number): void {
  const target = d.value
  if (target) target.status = String(v) as EndpointStatus
}

// ---------- 请求参数定义（Params / Headers / Body） ----------

type ReqTabKey = 'params' | 'headers' | 'body'

const reqTab = ref<ReqTabKey>('params')

const reqTabs = computed<TabItem[]>(() => [
  { key: 'params', label: 'Params', count: d.value?.request.params.length ?? 0 },
  { key: 'headers', label: 'Headers', count: d.value?.request.headers.length ?? 0 },
  { key: 'body', label: 'Body' },
])

function writeRows(list: KeyValue[], rows: KeyValue[]): void {
  list.splice(0, list.length, ...rows)
}

function onParamsUpdate(rows: KeyValue[]): void {
  if (d.value) writeRows(d.value.request.params, rows)
}

function onHeadersUpdate(rows: KeyValue[]): void {
  if (d.value) writeRows(d.value.request.headers, rows)
}

/** Form Data 字段表更新（urlencoded 容器写回）。 */
function onFormUpdate(rows: KeyValue[]): void {
  const target = d.value
  if (!target) return
  const body = target.request.body
  if (body.mode === 'urlencoded') writeRows(body.fields, rows)
}

/** Body 设计视图模式：仅支持 JSON / Form Data；其余类型引导到调试页。 */
type BodyViewMode = 'json' | 'form'

const bodyMode = computed<BodyViewMode | ''>(() => {
  const mode = d.value?.request.body.mode
  return mode === 'json' ? 'json' : mode === 'urlencoded' ? 'form' : ''
})

const BODY_MODE_OPTIONS = [
  { value: 'json', label: 'JSON' },
  { value: 'form', label: 'Form Data' },
]

/** 切换 Body 设计模式：json ↔ urlencoded（目标容器给空默认值，不搬移旧数据）。 */
function onBodyModeChange(v: string): void {
  const body = d.value?.request.body
  if (!body) return
  const current: BodyViewMode | '' =
    body.mode === 'json' ? 'json' : body.mode === 'urlencoded' ? 'form' : ''
  if (v === current) return
  if (v === 'json') {
    d.value!.request.body = { mode: 'json', raw: '{\n  \n}' }
  } else if (v === 'form') {
    d.value!.request.body = { mode: 'urlencoded', fields: [] }
  }
}

const BODY_VIEW_HINTS: Record<string, string> = {
  text: '当前为 Text Body，可在调试页编辑；切换上方模式可改为 JSON 定义',
  graphql: 'GraphQL Body 请在调试页编辑',
  multipart: 'multipart（含文件）Body 请在调试页编辑',
  binary: '二进制 Body 请在调试页配置文件路径',
  none: '尚未定义 Body，选择上方模式开始设计',
}

/** JSON 编辑区内容（直接写草稿 raw）。 */
const bodyRaw = computed({
  get: () => {
    const body = d.value?.request.body
    return body?.mode === 'json' ? body.raw : ''
  },
  set: (v: string) => {
    const body = d.value?.request.body
    if (body?.mode === 'json') body.raw = v
  },
})

const jsonValid = computed(() => {
  if (bodyMode.value !== 'json') return true
  const text = bodyRaw.value.trim()
  if (!text) return true
  try {
    JSON.parse(text)
    return true
  } catch {
    return false
  }
})

// ---------- 返回响应 (Responses)：复用响应示例存储 ----------

const responses = computed(() =>
  d.value ? (store.examples.get(d.value.id) ?? []) : [],
)

const expandedRespId = ref<string | null>(null)

/** 默认展开首个成功响应（无 2xx 时退回第一条），删除后自动收起。 */
watch(
  responses,
  (list) => {
    if (expandedRespId.value && !list.some((e) => e.id === expandedRespId.value)) {
      expandedRespId.value = null
    }
    if (!expandedRespId.value && list.length) {
      expandedRespId.value = (list.find((e) => e.status < 300) ?? list[0]).id
    }
  },
  { immediate: true },
)

/** 展开示例的编辑文本（未改动时回退格式化后的已存正文）。 */
const respEdits = ref<Map<string, string>>(new Map())

function respTextOf(ex: ResponseExample): string {
  const editing = respEdits.value.get(ex.id)
  if (editing !== undefined) return editing
  try {
    return JSON.stringify(JSON.parse(ex.body), null, 2)
  } catch {
    return ex.body
  }
}

/** 展示名：未命名时用标准状态码文案（200 OK / 404 Not Found…）。 */
function respDisplayName(ex: ResponseExample): string {
  return ex.name || `${ex.status} ${statusTextOf(ex.status)}`
}

function onRespEdit(ex: ResponseExample, v: string): void {
  respEdits.value.set(ex.id, v)
}

function toggleResp(id: string): void {
  expandedRespId.value = expandedRespId.value === id ? null : id
}

async function cacheExample(saved: ResponseExample): Promise<void> {
  if (!d.value) return
  const list = store.examples.get(d.value.id) ?? []
  const idx = list.findIndex((x) => x.id === saved.id)
  if (idx === -1) list.unshift(saved)
  else list[idx] = saved
  store.examples.set(d.value.id, [...list])
}

/** 新增状态码响应行（自定义输入）。 */
const newRespStatus = ref<number | null>(200)
const newRespName = ref('')

/** 标题行快捷创建按键：常用状态码一键建档。 */
const STATUS_PRESETS: { status: number; label: string }[] = [
  { status: 200, label: '+ 200 OK' },
  { status: 400, label: '+ 400 Bad Request' },
  { status: 500, label: '+ 500 Error' },
]

async function addResponse(preset?: number): Promise<void> {
  const endpointId = d.value?.id
  if (!endpointId) return
  // .number 修饰符在输入框清空时可能落回空串，统一 Number 归一。
  const status = Number(preset ?? newRespStatus.value)
  if (!status || status < 100 || status > 599) {
    toast.warning('状态码需在 100–599 之间')
    return
  }
  try {
    const now = new Date().toISOString()
    const saved = await api.saveExample({
      id: crypto.randomUUID(),
      endpoint_id: endpointId,
      name:
        newRespName.value.trim() ||
        `${status} ${preset !== undefined ? '响应' : 'Response'}`,
      status,
      headers: {},
      body: '',
      content_type: 'application/json',
      created_at: now,
      updated_at: now,
    })
    await cacheExample(saved)
    if (preset === undefined) {
      newRespStatus.value = 200
      newRespName.value = ''
    }
    expandedRespId.value = saved.id
    toast.success(`已添加 ${status} 响应`)
  } catch (err) {
    toast.error('添加失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

async function saveRespBody(ex: ResponseExample): Promise<void> {
  const text = respEdits.value.get(ex.id)
  if (text === undefined) return
  try {
    if (text.trim()) JSON.parse(text) // 仅校验，不强制格式化
  } catch {
    toast.warning('响应 Body 不是合法 JSON，仍可保存为文本')
  }
  try {
    const saved = await api.saveExample({ ...ex, body: text, updated_at: new Date().toISOString() })
    await cacheExample(saved)
    respEdits.value.delete(ex.id)
    toast.success(`已更新 ${ex.status} 响应`)
  } catch (err) {
    toast.error('保存失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

async function removeResp(ex: ResponseExample): Promise<void> {
  if (!d.value) return
  try {
    await store.removeExample(d.value.id, ex.id)
    if (expandedRespId.value === ex.id) expandedRespId.value = null
  } catch (err) {
    toast.error('删除失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

// ---------- 右侧实时预览：Schema 结构 / Mock 示例 ----------

type PreviewView = 'schema' | 'mock'

const previewView = ref<PreviewView>('schema')

const PREVIEW_VIEW_OPTIONS = [
  { value: 'schema', label: 'Schema' },
  { value: 'mock', label: 'Mock 示例' },
]

/** 叶子 Schema 行 → 类型标注（schema 视图）或示例值（mock 视图）。 */
function shapeOf(rows: SchemaRow[], mock: boolean): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const row of rows) {
    const key = row.name.trim()
    if (!key) continue
    if (row.type === 'object') {
      out[key] = shapeOf(row.children, mock)
    } else if (row.type === 'array') {
      out[key] =
        row.itemType === 'object' ? [shapeOf(row.children, mock)] : [`<${row.itemType ?? 'any'}>`]
    } else {
      out[key] = mock ? row.example || row.type : row.type
    }
  }
  return out
}

/** 样本文本 → schema / mock 双形态；不可解析返回 null。 */
function sampleShape(text: string, mock: boolean): Record<string, unknown> | null {
  const trimmed = text.trim()
  if (!trimmed) return null
  let parsed: unknown
  try {
    parsed = JSON.parse(trimmed)
  } catch {
    return null
  }
  const rows = inferSchema(parsed)
  if (mock) {
    return mockJsonFromSchema(rows)
  }
  const shaped = shapeOf(rows, false)
  return Object.keys(shaped).length ? shaped : null
}

function kvShape(list: KeyValue[]): Record<string, unknown>[] {
  return list
    .filter((kv) => kv.enabled && kv.key.trim())
    .map((kv) => ({
      name: kv.key,
      type: kv.field_type ?? 'string',
      required: kv.required ?? true,
      ...(kv.description ? { description: kv.description } : {}),
      ...(kv.example?.trim() ? { example: kv.example } : {}),
    }))
}

/** 表单类 Body（urlencoded / multipart）→ 扁平对象形态。 */
function formBodyShape(fields: KeyValue[] | MultipartField[], mock: boolean): Record<string, unknown> {
  const obj: Record<string, unknown> = {}
  for (const f of fields) {
    if (!f.enabled || !f.key.trim()) continue
    const isFile = 'value_type' in f && f.value_type === 'file_path'
    obj[f.key] = mock ? f.value || `<${isFile ? 'file' : 'string'}>` : 'string'
  }
  return obj
}

function bodyShape(mock: boolean): unknown {
  const body = d.value?.request.body
  if (!body) return undefined
  if (body.mode === 'json') return sampleShape(body.raw, mock) ?? '<invalid-json>'
  if (body.mode === 'urlencoded') return formBodyShape(body.fields, mock)
  if (body.mode === 'multipart') return formBodyShape(body.fields, mock)
  if (!mock) return `<${body.mode}>`
  return undefined
}

/** 预览对象：标准 API 定义结构，随草稿实时刷新。 */
const preview = computed<Record<string, unknown> | null>(() => {
  const target = d.value
  if (!target) return null
  const mock = previewView.value === 'mock'
  const req: Record<string, unknown> = {}
  const query = kvShape(target.request.params)
  const headers = kvShape(target.request.headers)
  if (query.length) req.query = query
  if (headers.length) req.headers = headers
  const body = bodyShape(mock)
  if (body !== undefined) req.body = body

  const responsesShape: Record<string, unknown> = {}
  for (const ex of responses.value) {
    responsesShape[String(ex.status)] = sampleShape(ex.body, mock) ?? { example: ex.body || '<empty>' }
  }

  const out: Record<string, unknown> = {
    method: target.method,
    path: target.path,
  }
  if (Object.keys(req).length) out.request = req
  if (Object.keys(responsesShape).length) out.responses = responsesShape
  return out
})

const previewHtml = computed(() => {
  if (!preview.value) return ''
  return highlightJSON(JSON.stringify(preview.value, null, 2))
})

/** 复制按钮反馈态：Tooltip 短暂切换为「已复制」。 */
const copied = ref(false)
let copiedTimer: ReturnType<typeof setTimeout> | null = null

async function copyPreview(): Promise<void> {
  if (!preview.value) return
  try {
    await navigator.clipboard.writeText(JSON.stringify(preview.value, null, 2))
    copied.value = true
    if (copiedTimer) clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => {
      copied.value = false
    }, 1600)
  } catch {
    toast.error('复制失败')
  }
}

onBeforeUnmount(() => {
  if (copiedTimer) clearTimeout(copiedTimer)
})
</script>

<template>
  <div v-if="d" class="design">
    <!-- ---- 顶部操作栏：面包屑上下文 + 保存 ---- -->
    <header class="topbar doc-card">
      <div class="topbar-crumb">
        <span class="method-pill" :class="`mp-${d.method.toLowerCase()}`">{{ d.method }}</span>
        <code class="crumb-path">{{ d.path }}</code>
        <span class="crumb-sep">/</span>
        <span class="crumb-label">接口设计</span>
      </div>
      <div class="topbar-actions">
        <span v-if="dirty" class="dirty-hint" title="内容尚未保存">
          <span class="dirty-dot"></span>
          未保存
        </span>
        <button type="button" class="save-btn" @click="emit('save')">
          <Icon name="save" :size="13" />
          保存设计
        </button>
      </div>
    </header>

    <div class="design-body">
      <!-- ---- 左：设计主体 ---- -->
      <div class="design-main">
        <!-- 基本信息 -->
        <section class="doc-card blk">
          <h4 class="doc-sec-title">基本信息</h4>
          <div class="grid2">
            <label class="fld">
              <span class="fld-label">接口名称</span>
              <input v-model="d.name" class="rf-input" placeholder="例如：获取用户列表" spellcheck="false" />
            </label>
            <label class="fld">
              <span class="fld-label">生命周期状态</span>
              <CustomSelect
                :model-value="d.status"
                :options="STATUS_OPTIONS"
                @update:model-value="onStatusChange"
              />
            </label>
          </div>

          <label class="fld">
            <span class="fld-label">请求路径</span>
            <div class="path-group">
              <CustomSelect
                class="method-select"
                :model-value="d.method"
                :options="METHOD_OPTIONS"
                @update:model-value="onMethodChange"
              >
                <template #display="{ label }">
                  <span class="method-label" :class="`mp-${d.method.toLowerCase()}`">{{ label }}</span>
                </template>
              </CustomSelect>
              <span class="pg-divider"></span>
              <input v-model="d.path" class="path-input" placeholder="/api/v1/resource" spellcheck="false" />
            </div>
          </label>

          <label class="fld">
            <span class="fld-label">接口描述</span>
            <textarea
              v-model="d.description"
              class="desc-area"
              rows="3"
              placeholder="接口用途、注意事项、返回约定…"
              spellcheck="false"
            ></textarea>
          </label>
        </section>

        <!-- 请求定义 -->
        <section class="doc-card blk">
          <h4 class="doc-sec-title">请求定义 (Request)</h4>
          <Tabs v-model="reqTab" :tabs="reqTabs" size="sm" />

          <template v-if="reqTab === 'params'">
            <ParamDefineTable :rows="d.request.params" key-placeholder="参数名" @update:model-value="onParamsUpdate" />
          </template>
          <template v-else-if="reqTab === 'headers'">
            <ParamDefineTable :rows="d.request.headers" key-placeholder="Header 名" @update:model-value="onHeadersUpdate" />
          </template>

          <!-- Body 设计器 -->
          <template v-else>
            <div class="body-head">
              <SegmentedControl
                size="sm"
                :options="BODY_MODE_OPTIONS"
                :model-value="bodyMode || null"
                @update:model-value="onBodyModeChange($event)"
              />
              <span v-if="bodyMode === 'json'" class="json-state" :class="{ bad: !jsonValid }">
                {{ jsonValid ? '✓ 合法 JSON' : '✕ JSON 解析失败' }}
              </span>
            </div>

            <p v-if="!bodyMode" class="body-hint">{{ BODY_VIEW_HINTS[d.request.body.mode] ?? '' }}</p>

            <textarea
              v-if="bodyMode === 'json'"
              v-model="bodyRaw"
              class="body-json mono"
              spellcheck="false"
              placeholder='{ "field": "value" }'
            ></textarea>

            <ParamDefineTable
              v-else-if="bodyMode === 'form'"
              :rows="d.request.body.mode === 'urlencoded' ? d.request.body.fields : []"
              key-placeholder="字段名"
              :show-example="false"
              @update:model-value="onFormUpdate"
            />
          </template>
        </section>

        <!-- 返回响应 -->
        <section class="doc-card blk">
          <div class="resp-head">
            <h4 class="doc-sec-title">返回响应 (Responses)</h4>
            <span class="resp-count">{{ responses.length }}</span>
            <span class="resp-head-spacer"></span>
            <div class="resp-presets">
              <button
                v-for="p in STATUS_PRESETS"
                :key="p.status"
                type="button"
                class="resp-preset"
                :class="{ err: p.status >= 400 }"
                @click="addResponse(p.status)"
              >
                {{ p.label }}
              </button>
            </div>
          </div>

          <div v-if="responses.length" class="resp-list">
            <div v-for="ex in responses" :key="ex.id" class="resp-item">
              <button type="button" class="resp-row" @click="toggleResp(ex.id)">
                <Icon
                  name="chevron-right"
                  :size="12"
                  class="resp-caret"
                  :class="{ open: expandedRespId === ex.id }"
                />
                <span class="resp-status" :class="{ err: ex.status >= 400 }">{{ ex.status }}</span>
                <span class="resp-name">{{ respDisplayName(ex) }}</span>
                <span class="resp-meta">{{ ex.updated_at.slice(0, 16).replace('T', ' ') }}</span>
              </button>
              <div v-if="expandedRespId === ex.id" class="resp-editor">
                <textarea
                  class="body-json mono"
                  :value="respTextOf(ex)"
                  spellcheck="false"
                  placeholder='响应 Body 示例，例如 { "code": 0 }'
                  @input="onRespEdit(ex, ($event.target as HTMLTextAreaElement).value)"
                ></textarea>
                <div class="resp-actions">
                  <button
                    type="button"
                    class="rf-btn rf-btn-sm"
                    :disabled="!respEdits.has(ex.id)"
                    @click="saveRespBody(ex)"
                  >
                    <Icon name="save" :size="12" /> 保存修改
                  </button>
                  <Popconfirm :title="`删除 ${ex.status} 响应？`" @confirm="removeResp(ex)">
                    <IconButton name="trash" :size="13" tone="danger" title="删除该响应" />
                  </Popconfirm>
                </div>
              </div>
            </div>
          </div>
          <p v-else class="resp-empty">尚未定义响应，点右上角快捷键或自定义状态码开始设计。</p>
          <div class="resp-add">
            <input
              v-model.number="newRespStatus"
              class="rf-input resp-status-input mono"
              type="number"
              min="100"
              max="599"
              title="自定义状态码"
              placeholder="状态码"
            />
            <input
              v-model="newRespName"
              class="rf-input resp-name-input"
              placeholder="自定义响应名称（可选）"
              spellcheck="false"
              @keyup.enter="addResponse()"
            />
            <button type="button" class="rf-btn rf-btn-sm" @click="addResponse()">
              <Icon name="plus" :size="12" /> 添加
            </button>
          </div>
        </section>
      </div>

      <!-- ---- 右：实时预览 / Mock 生成 ---- -->
      <aside class="preview doc-card">
        <div class="preview-head">
          <h4 class="doc-sec-title">实时预览</h4>
          <SegmentedControl
            class="preview-pills"
            :model-value="previewView"
            size="sm"
            :options="PREVIEW_VIEW_OPTIONS"
            @update:model-value="previewView = $event as PreviewView"
          />
          <Tooltip :content="copied ? '已复制' : '复制 Schema'" placement="bottom">
            <IconButton name="copy" :size="13" @click="copyPreview" />
          </Tooltip>
        </div>
        <pre v-if="previewHtml" class="preview-code mono" v-html="previewHtml"></pre>
        <p v-else class="preview-empty">左侧填写接口定义后，此处将实时生成标准 JSON 预览。</p>
        <p class="preview-note">
          {{ previewView === 'mock' ? 'Mock 示例由 Schema 推断生成（每个字段唯一），可直接用作响应示例。' : 'Schema 视图为字段类型结构，供文档 / Mock 生成消费。' }}
        </p>
      </aside>
    </div>
  </div>
</template>

<style scoped>
.design {
  display: flex;
  flex-direction: column;
  gap: 14px;
  max-width: 1280px;
}

/* ---- 顶部操作栏 ---- */

.topbar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 14px;
  position: sticky;
  top: 0;
  z-index: 5;
  backdrop-filter: blur(6px);
}

.topbar-crumb {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1;
}

.crumb-path {
  font-family: var(--font-mono);
  font-size: var(--fs-md);
  font-weight: 600;
  color: var(--text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.crumb-sep {
  color: var(--text-3);
}

.crumb-label {
  font-size: 12px;
  color: var(--text-3);
  white-space: nowrap;
}

.topbar-actions {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-shrink: 0;
}

/* Method 徽标（顶栏 & 路径组共用） */
.method-pill {
  flex-shrink: 0;
  padding: 2px 9px;
  border-radius: 6px;
  font-family: var(--font-mono);
  font-size: 11.5px;
  font-weight: 700;
  letter-spacing: 0.04em;
}
.mp-get,
.mp-head {
  color: var(--get);
  background: color-mix(in srgb, var(--get) 10%, transparent);
}
.mp-post {
  color: var(--post);
  background: color-mix(in srgb, var(--post) 10%, transparent);
}
.mp-put {
  color: var(--put);
  background: color-mix(in srgb, var(--put) 10%, transparent);
}
.mp-delete {
  color: var(--delete);
  background: color-mix(in srgb, var(--delete) 10%, transparent);
}
.mp-patch {
  color: var(--patch);
  background: color-mix(in srgb, var(--patch) 12%, transparent);
}
.mp-options {
  color: var(--text-2);
  background: var(--bg-hover);
}

/* 未保存提醒：黄色小点 + 文本 */
.dirty-hint {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11.5px;
  color: var(--warning);
}

.dirty-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--warning);
  box-shadow: 0 0 6px color-mix(in srgb, var(--warning) 60%, transparent);
}

/* 主保存按钮：主题紫实底 */
.save-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 16px;
  border: none;
  border-radius: 8px;
  background: var(--accent);
  color: #fff;
  font-family: inherit;
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
  box-shadow: var(--shadow-sm);
  transition:
    background var(--dur) var(--ease),
    transform var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}
.save-btn:hover {
  background: var(--accent-hover);
  box-shadow: var(--shadow);
}
.save-btn:active {
  transform: translateY(1px);
}
.save-btn:focus-visible {
  outline: 2px solid var(--focus-ring);
  outline-offset: 2px;
}

/* ---- 双栏布局 ---- */

.design-body {
  display: grid;
  grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
  gap: 16px;
  align-items: start;
}

@media (max-width: 1080px) {
  .design-body {
    grid-template-columns: 1fr;
  }
}

.design-main {
  display: flex;
  flex-direction: column;
  /* space-y-6：卡片间保留充足呼吸感 */
  gap: 24px;
  min-width: 0;
}

.blk {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px 18px;
  border-radius: 10px;
}

/* ---- 基本信息 ---- */

.grid2 {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
}

@media (max-width: 720px) {
  .grid2 {
    grid-template-columns: 1fr;
  }
}

.fld {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.fld-label {
  font-size: 11.5px;
  color: var(--text-3);
}

/* Method + Path 组合输入组 */
.path-group {
  display: flex;
  align-items: stretch;
  height: 32px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius);
  overflow: hidden;
  transition:
    border-color var(--dur) var(--ease),
    box-shadow var(--dur) var(--ease);
}
.path-group:focus-within {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-tint);
}

.path-group .method-select {
  width: 104px;
  flex-shrink: 0;
  background: var(--bg-panel);
}
.path-group .method-select :deep(.cs-trigger) {
  height: 100%;
  border: none;
  background: transparent;
  box-shadow: none;
  border-radius: 0;
  font-weight: 700;
}

.method-label {
  font-weight: 700;
}
.method-label.mp-get {
  color: var(--get);
}
.method-label.mp-post {
  color: var(--post);
}
.method-label.mp-put {
  color: var(--put);
}
.method-label.mp-delete {
  color: var(--delete);
}
.method-label.mp-patch {
  color: var(--patch);
}
.method-label.mp-options,
.method-label.mp-head {
  color: var(--text-2);
}

.pg-divider {
  width: 1px;
  flex-shrink: 0;
  background: var(--border);
}

.path-input {
  flex: 1;
  min-width: 0;
  padding: 0 10px;
  border: none;
  outline: none;
  background: #0a0a0a;
  color: var(--text-1);
  font-family: var(--font-mono);
  font-size: 13px;
}
html[data-theme='light'] .path-input {
  background: var(--bg-code);
}

.desc-area {
  width: 100%;
  padding: 7px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-code);
  color: var(--text-1);
  font-family: inherit;
  font-size: 12.5px;
  line-height: 1.6;
  resize: vertical;
  transition: border-color var(--dur) var(--ease);
}
.desc-area:focus {
  outline: none;
  border-color: var(--accent);
}

/* ---- 请求定义 ---- */

.body-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.json-state {
  font-size: 11.5px;
  color: var(--success);
}
.json-state.bad {
  color: var(--danger);
}

.body-hint {
  margin: 0;
  padding: 12px;
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius);
  font-size: 12px;
  color: var(--text-3);
}

.mono {
  font-family: var(--font-mono);
}

.body-json {
  width: 100%;
  min-height: 150px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: #0a0a0a;
  color: var(--text-1);
  font-size: 12px;
  line-height: 1.6;
  resize: vertical;
  transition: border-color var(--dur) var(--ease);
}
html[data-theme='light'] .body-json {
  background: var(--bg-code);
}
.body-json:focus {
  outline: none;
  border-color: var(--accent);
}

/* ---- 返回响应 ---- */

.resp-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.resp-count {
  font-size: 11.5px;
  color: var(--text-3);
}

.resp-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.resp-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.resp-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-panel);
  color: var(--text-2);
  font-size: 12.5px;
  cursor: pointer;
  text-align: left;
  transition:
    border-color var(--dur) var(--ease),
    background var(--dur) var(--ease);
}
.resp-row:hover {
  border-color: var(--border-strong);
  background: var(--bg-hover);
}

.resp-caret {
  flex-shrink: 0;
  color: var(--text-3);
  transition: transform var(--dur) var(--ease);
}
.resp-caret.open {
  transform: rotate(90deg);
}

.resp-status {
  font-family: var(--font-mono);
  font-weight: 700;
  font-size: 11.5px;
  color: var(--success);
}
.resp-status.err {
  color: var(--danger);
}

.resp-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-1);
}

.resp-meta {
  flex-shrink: 0;
  font-size: 11px;
  color: var(--text-3);
}

.resp-editor {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.resp-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.resp-empty {
  margin: 0;
  font-size: 12px;
  color: var(--text-3);
}

.resp-add {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
  padding-top: 4px;
  border-top: 1px solid var(--border);
}

.resp-add-label {
  font-size: 11.5px;
  color: var(--text-3);
}

.resp-preset {
  padding: 2px 9px;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: var(--bg-panel);
  color: var(--success);
  font-family: var(--font-mono);
  font-size: 11px;
  cursor: pointer;
  transition:
    border-color var(--dur) var(--ease),
    background var(--dur) var(--ease);
}
.resp-preset.err {
  color: var(--danger);
}
.resp-preset:hover {
  border-color: var(--accent);
  background: var(--accent-tint);
}

.resp-status-input {
  width: 76px;
  height: 26px;
}

.resp-name-input {
  flex: 1;
  min-width: 120px;
  height: 26px;
}

/* ---- 右侧实时预览 ---- */

.preview {
  display: flex;
  flex-direction: column;
  gap: 10px;
  position: sticky;
  top: 62px;
  border-radius: 10px;
}

.preview-head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.preview-head h4 {
  flex: 1;
  margin: 0;
}

.preview-code {
  margin: 0;
  max-height: 520px;
  overflow: auto;
  padding: 12px;
  border: 1px solid rgba(38, 38, 38, 0.8);
  border-radius: 8px;
  background: #0a0a0a;
  font-size: 12px;
  line-height: 1.65;
  color: var(--text-1);
  white-space: pre-wrap;
  word-break: break-all;
}

.preview-empty {
  margin: 0;
  padding: 20px 12px;
  border: 1px dashed var(--border-strong);
  border-radius: 8px;
  font-size: 12px;
  color: var(--text-3);
  text-align: center;
}

.preview-note {
  margin: 0;
  font-size: 11px;
  color: var(--text-3);
  line-height: 1.5;
}
</style>
