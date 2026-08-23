<script setup lang="ts">
/**
 * ExportDocsDialog：文档导出弹窗（文档预览页右上角入口）。
 *
 * - 范围：仅当前接口 / 整个项目（已废弃接口后端统一排除）；
 * - 格式卡片单选：OpenAPI 3.0 (JSON/YAML) / Postman v2.1 / Markdown /
 *   HTML 离线单页 / cURL 脚本；
 * - 流程：export_docs 生成内容 → @tauri-apps/plugin-dialog 原生保存框
 *   （默认文件名 rustfox-api-{项目}-{日期}.{ext}）→ save_text_file 落盘
 *   → 成功 Toast 附「打开文件位置」（opener.revealItemInDir）。
 */
import { computed, ref } from 'vue'
import { save } from '@tauri-apps/plugin-dialog'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import Modal from '../ui/Modal.vue'
import Icon from '../ui/Icon.vue'
import { useWorkspaceStore } from '../../stores/workspace'
import { useFoxApi } from '../../composables/useFoxApi'
import { useToast } from '../../composables/useToast'
import type { Endpoint, ExportFormat, ExportedDoc } from '../../types/foxApi'

const props = defineProps<{
  /** 仅导出当前接口时使用。 */
  draft: Endpoint | null
}>()

const emit = defineEmits<{ close: [] }>()

const store = useWorkspaceStore()
const api = useFoxApi()
const toast = useToast()

// ---------- 范围 ----------

type Scope = 'current' | 'project'

const scope = ref<Scope>('current')

const canExportProject = computed(() => !!store.project)
const projectName = computed(() => store.project?.name ?? '')

// ---------- 格式卡片 ----------

interface FormatCard {
  key: string
  format: Exclude<ExportFormat, 'openapi_json' | 'openapi_yaml'> | 'openapi'
  title: string
  desc: string
  icon: 'package' | 'list' | 'file' | 'code' | 'terminal'
}

const FORMAT_CARDS: FormatCard[] = [
  {
    key: 'openapi',
    format: 'openapi',
    title: 'OpenAPI 3.0 (JSON/YAML)',
    desc: '推荐 · 可直接导入 Postman / Swagger / Apifox',
    icon: 'package',
  },
  {
    key: 'postman',
    format: 'postman',
    title: 'Postman Collection v2.1',
    desc: '给前端/测试同学一键导入 Postman 调试',
    icon: 'list',
  },
  {
    key: 'markdown',
    format: 'markdown',
    title: 'Markdown (.md)',
    desc: '适合放入 Git 仓库或 Notion / 语雀技术文档',
    icon: 'file',
  },
  {
    key: 'html',
    format: 'html',
    title: 'HTML 离线单页',
    desc: '开箱即用的美观离线网页文档，支持离线预览',
    icon: 'code',
  },
  {
    key: 'curl',
    format: 'curl_script',
    title: 'cURL 命令脚本 (.sh)',
    desc: '一键导出所有接口的可执行 cURL 脚本',
    icon: 'terminal',
  },
]

const selectedKey = ref('openapi')
/** OpenAPI 卡片选中时的子格式。 */
const openapiVariant = ref<'openapi_json' | 'openapi_yaml'>('openapi_json')

const selectedFormat = computed<FormatCard | undefined>(() =>
  FORMAT_CARDS.find((c) => c.key === selectedKey.value),
)

const resolvedFormat = computed<ExportFormat>(() => {
  if (selectedKey.value === 'openapi') return openapiVariant.value
  return selectedFormat.value?.format as ExportFormat
})

// ---------- 导出流程 ----------

const exporting = ref(false)
/** 导出成功后的落盘路径（供「打开文件位置」）。 */
const savedPath = ref<string | null>(null)

async function startExport(): Promise<void> {
  const projectId = store.project?.id
  if (!projectId) return
  if (scope.value === 'current' && !props.draft) return

  exporting.value = true
  savedPath.value = null
  try {
    // 1) 后端生成内容 + 建议文件名
    const doc: ExportedDoc = await api.exportDocs({
      projectId,
      endpointId: scope.value === 'current' ? props.draft!.id : null,
      format: resolvedFormat.value,
    })

    // 2) 原生保存框（Tauri dialog 插件）
    const path = await save({
      defaultPath: doc.suggested_name,
      filters: [{ name: 'API 文档', extensions: [doc.suggested_name.split('.').pop() ?? 'txt'] }],
    })
    if (!path) return // 用户取消

    // 3) 写入磁盘
    await api.writeTextFile(path, doc.content)
    savedPath.value = path

    toast.success('✓ 文档导出成功！', {
      message: path.split('/').pop() || path,
      action: {
        label: '打开文件位置',
        run: () => {
          void revealItemInDir(path).catch(() => toast.error('无法定位文件'))
        },
      },
    })
    emit('close')
  } catch (err) {
    toast.error('导出失败', { message: err instanceof Error ? err.message : String(err) })
  } finally {
    exporting.value = false
  }
}
</script>

<template>
  <Modal :open="true" title="导出 API 文档" width="560px" @close="emit('close')">
    <!-- 范围 -->
    <div class="sec">
      <p class="sec-label">导出范围</p>
      <div class="scope-grid">
        <button
          type="button"
          class="scope-card"
          :class="{ active: scope === 'current' }"
          @click="scope = 'current'"
        >
          <span class="scope-title">当前接口</span>
          <span class="scope-desc">{{ draft?.name || '未选择接口' }}</span>
        </button>
        <button
          type="button"
          class="scope-card"
          :class="{ active: scope === 'project', disabled: !canExportProject }"
          :disabled="!canExportProject"
          @click="canExportProject && (scope = 'project')"
        >
          <span class="scope-title">整个项目</span>
          <span class="scope-desc">
            {{ canExportProject ? `${projectName} 全部接口` : '无激活项目' }}
          </span>
        </button>
      </div>
    </div>

    <!-- 格式卡片 Grid -->
    <div class="sec">
      <p class="sec-label">导出格式</p>
      <div class="fmt-grid">
        <button
          v-for="c in FORMAT_CARDS"
          :key="c.key"
          type="button"
          class="fmt-card"
          :class="{ active: selectedKey === c.key }"
          @click="selectedKey = c.key"
        >
          <Icon :name="c.icon" :size="15" class="fmt-icon" />
          <span class="fmt-body">
            <span class="fmt-title">{{ c.title }}</span>
            <span class="fmt-desc">{{ c.desc }}</span>
          </span>
          <Icon v-if="selectedKey === c.key" name="check" :size="14" class="fmt-check" />
        </button>
      </div>

      <!-- OpenAPI 子格式切换 -->
      <div v-if="selectedKey === 'openapi'" class="variant-row">
        <span class="sec-label">序列化</span>
        <div class="variant-toggle">
          <button
            type="button"
            class="variant-btn"
            :class="{ active: openapiVariant === 'openapi_json' }"
            @click="openapiVariant = 'openapi_json'"
          >
            JSON
          </button>
          <button
            type="button"
            class="variant-btn"
            :class="{ active: openapiVariant === 'openapi_yaml' }"
            @click="openapiVariant = 'openapi_yaml'"
          >
            YAML
          </button>
        </div>
      </div>
    </div>

    <template #footer>
      <button type="button" class="rf-btn" @click="emit('close')">取消</button>
      <button type="button" class="rf-btn rf-btn-primary" :disabled="exporting" @click="startExport">
        <Icon name="download" :size="13" />
        {{ exporting ? '正在生成…' : '开始导出' }}
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.sec {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.sec + .sec {
  margin-top: 16px;
}

.sec-label {
  margin: 0;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-3);
  text-transform: uppercase;
  letter-spacing: 0.4px;
}

/* ---- 范围双卡 ---- */
.scope-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.scope-card {
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-panel);
  color: inherit;
  text-align: left;
  cursor: pointer;
  transition:
    border-color var(--dur) var(--ease),
    background var(--dur) var(--ease);
}
.scope-card:hover:not(.disabled) {
  border-color: var(--border-strong);
}
.scope-card.active {
  border-color: var(--accent);
  background: var(--accent-tint);
}
.scope-card.disabled {
  opacity: 0.45;
  cursor: default;
}

.scope-title {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-1);
}
.scope-desc {
  font-size: 11.5px;
  color: var(--text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* ---- 格式卡片 Grid ---- */
.fmt-grid {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.fmt-card {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 9px 12px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-panel);
  text-align: left;
  cursor: pointer;
  transition:
    border-color var(--dur) var(--ease),
    background var(--dur) var(--ease);
}
.fmt-card:hover {
  border-color: var(--border-strong);
  background: var(--bg-hover);
}
.fmt-card.active {
  border-color: var(--accent);
  background: var(--accent-tint);
}

.fmt-icon {
  flex-shrink: 0;
  color: var(--text-2);
}
.fmt-card.active .fmt-icon {
  color: var(--accent);
}

.fmt-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.fmt-title {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-1);
}

.fmt-desc {
  font-size: 11px;
  color: var(--text-3);
}

.fmt-check {
  flex-shrink: 0;
  color: var(--accent);
}

/* ---- OpenAPI 序列化子选项 ---- */
.variant-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.variant-toggle {
  display: inline-flex;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: #0a0a0a;
}

.variant-btn {
  height: 22px;
  padding: 0 12px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text-2);
  font-family: var(--font-mono);
  font-size: 11px;
  cursor: pointer;
  transition:
    background var(--dur) var(--ease),
    color var(--dur) var(--ease);
}
.variant-btn.active {
  background: var(--accent);
  color: #fff;
}
</style>
