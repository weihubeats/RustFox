<script setup lang="ts">
/**
 * ImportDialog：OpenAPI/Swagger/Postman 文档导入。
 * 粘贴文本或选择文件 → 后端解析预览 → 确认后落库。
 *
 * - mode="workspace"（默认）：导入到当前激活项目（工作区内使用）；
 * - mode="new-project"：先创建新项目并激活，再写入接口（仪表板「导入项目」）。
 */
import { computed, onMounted, ref } from 'vue'
import { useWorkspaceStore } from '../stores/workspace'
import { useFoxApi } from '../composables/useFoxApi'
import { useToast } from '../composables/useToast'
import Modal from './ui/Modal.vue'
import type { ImportedEndpoint, ImportFormat, Project } from '../types/foxApi'

const props = withDefaults(
  defineProps<{
    /** workspace：写入当前激活项目；new-project：创建新项目承接导入。 */
    mode?: 'workspace' | 'new-project'
    /** 预填文档文本（拖拽导入：Dropzone 读入文件后带内容打开本弹窗）。 */
    initialText?: string
  }>(),
  { mode: 'workspace' },
)

const emit = defineEmits<{ close: []; imported: [project: Project] }>()

const store = useWorkspaceStore()
const api = useFoxApi()
const toast = useToast()

const text = ref('')
const busy = ref(false)
const result = ref<{ format: ImportFormat; endpoints: ImportedEndpoint[] } | null>(null)
const fileInput = ref<HTMLInputElement | null>(null)

/** new-project 模式下的目标项目名。 */
const projectName = ref('')

const FORMAT_LABEL: Record<ImportFormat, string> = {
  openapi30: 'OpenAPI 3.0',
  openapi31: 'OpenAPI 3.1（已转换为 3.0 子集）',
  swagger20: 'Swagger 2.0',
  postman21: 'Postman 集合 v2.1',
  unknown: '无法识别',
}

async function pickFile(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  text.value = await file.text()
  parse()
}

async function parse(): Promise<void> {
  if (!text.value.trim()) return
  busy.value = true
  try {
    result.value = await api.importDocument(text.value)
    toast.success(`识别为 ${FORMAT_LABEL[result.value.format]}，共 ${result.value.endpoints.length} 个接口`)
  } catch (err) {
    result.value = null
    toast.error('解析失败', { message: err instanceof Error ? err.message : String(err) })
  } finally {
    busy.value = false
  }
}

// 拖拽导入入口：带预填内容打开时立即解析
onMounted(() => {
  if (props.initialText) {
    text.value = props.initialText
    void parse()
  }
})

async function confirm(): Promise<void> {
  if (!result.value) return
  busy.value = true
  try {
    if (props.mode === 'new-project') {
      // 先建项目并激活（switchProject 会加载其环境/接口缓存），再写入接口
      const now = new Date().toISOString()
      const name =
        projectName.value.trim() ||
        `导入项目 ${new Date().toLocaleDateString('zh-CN')}`
      const project = await api.saveProject({
        id: crypto.randomUUID(),
        name,
        description: `由 ${FORMAT_LABEL[result.value.format]} 导入`,
        variables: {},
        created_at: now,
        updated_at: now,
      })
      await store.switchProject(project.id)
      const summary = await store.importEndpoints(result.value.endpoints)
      toast.success(`已创建「${project.name}」并导入 ${summary.endpoints} 个接口`)
      emit('imported', project)
    } else {
      const summary = await store.importEndpoints(result.value.endpoints)
      toast.success(`已导入 ${summary.endpoints} 个接口（含 ${summary.examples} 个示例）`)
    }
    emit('close')
  } catch (err) {
    toast.error('导入失败', { message: err instanceof Error ? err.message : String(err) })
  } finally {
    busy.value = false
  }
}

/** 确认按钮可用性：workspace 模式需激活项目；两种模式都要求已解析出接口。 */
const canConfirm = computed(
  () => !!result.value && !busy.value && (props.mode === 'new-project' || !!store.project),
)
</script>

<template>
  <Modal :open="true" :title="mode === 'new-project' ? '导入为新项目' : '导入文档'" width="560px" @close="emit('close')">
    <p class="import-hint">
      {{
        mode === 'new-project'
          ? '将解析结果写入一个新建项目（自动设为激活）。支持 OpenAPI / Swagger / Postman。'
          : '支持 OpenAPI 3.0 / Swagger 2.0 / Postman Collection v2.1，自动识别格式，导入到当前项目。'
      }}
    </p>

    <label v-if="mode === 'new-project'" class="import-name-row">
      <span class="import-name-label">新项目名称</span>
      <input
        v-model="projectName"
        class="rf-input"
        placeholder="例如：支付网关"
        spellcheck="false"
      />
    </label>

    <textarea
      v-model="text"
      class="rf-input import-text"
      spellcheck="false"
      placeholder="粘贴 OpenAPI / Swagger / Postman JSON…"
    ></textarea>
    <div class="import-tools">
      <button class="rf-btn rf-btn-sm" type="button" :disabled="busy" @click="fileInput?.click()">
        选择文件
      </button>
      <button class="rf-btn rf-btn-sm" type="button" :disabled="busy || !text.trim()" @click="parse">
        {{ busy ? '解析中…' : '解析' }}
      </button>
      <input ref="fileInput" type="file" accept=".json,.yaml,.yml,application/json" class="import-file" @change="pickFile" />
    </div>

    <div v-if="result" class="import-preview">
      <p class="import-hint">
        {{ FORMAT_LABEL[result.format] }}：{{ result.endpoints.length }} 个接口
        （{{ result.endpoints.filter((e) => e.folder_hint).length }} 个按分组建文件夹）
      </p>
      <ul class="import-list">
        <li v-for="(ep, i) in result.endpoints.slice(0, 12)" :key="i" class="import-row">
          <span class="import-method">{{ ep.method }}</span>
          <span class="import-path">{{ ep.path }}</span>
        </li>
        <li v-if="result.endpoints.length > 12" class="import-hint">… 其余 {{ result.endpoints.length - 12 }} 个略</li>
      </ul>
    </div>

    <template #footer>
      <button
        v-if="result"
        class="rf-btn rf-btn-primary rf-btn-sm"
        type="button"
        :disabled="!canConfirm"
        @click="confirm"
      >
        {{ mode === 'new-project' ? '创建并导入' : '确认导入' }}
      </button>
      <button class="rf-btn rf-btn-sm" type="button" @click="emit('close')">取消</button>
    </template>
  </Modal>
</template>

<style scoped>
.import-hint {
  margin: 0;
  font-size: 12.5px;
  color: var(--text-2);
}

.import-name-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 12px;
}
.import-name-label {
  flex-shrink: 0;
  font-size: 12px;
  color: var(--text-3);
}
.import-name-row .rf-input {
  flex: 1;
  height: 30px;
}

.import-text {
  width: 100%;
  min-height: 140px;
  margin-top: 10px;
  font-family: var(--font-mono);
  font-size: 12px;
  resize: vertical;
}

.import-tools {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-top: 8px;
}

.import-preview {
  margin-top: 12px;
}

.import-list {
  margin: 8px 0 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.import-row {
  display: flex;
  gap: 10px;
  font-size: 12.5px;
}

.import-method {
  width: 56px;
  flex-shrink: 0;
  font-weight: 700;
  color: var(--text-2);
}

.import-path {
  font-family: var(--font-mono);
  color: var(--text-1);
  word-break: break-all;
}

.import-file {
  display: none;
}
</style>