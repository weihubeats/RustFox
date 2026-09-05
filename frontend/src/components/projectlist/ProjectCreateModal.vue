<script setup lang="ts">
/**
 * ProjectCreateModal：新建 API 项目弹窗。
 *
 * - 名称必填（不超过 50 字符）、描述可选（不超过 200 字符）；
 * - 创建成功后 emit('created', project)，由父级并入列表。
 */
import { ref, watch } from 'vue'
import Modal from '../ui/Modal.vue'
import { useFoxApi } from '../../composables/useFoxApi'
import { useToast } from '../../composables/useToast'
import { useLocaleStore } from '../../stores/locale'
import type { Project } from '../../types/foxApi'

const NAME_MAX = 50
const DESC_MAX = 200

const api = useFoxApi()
const toast = useToast()
const locale = useLocaleStore()
const t = locale.t

const open = defineModel<boolean>('open', { default: false })

const newName = ref('')
const newDesc = ref('')
const createError = ref<string | null>(null)

// 每次打开时重置表单
watch(open, (v) => {
  if (v) {
    newName.value = ''
    newDesc.value = ''
    createError.value = null
  }
})

const emit = defineEmits<{ created: [project: Project] }>()

async function confirmCreate(): Promise<void> {
  const name = newName.value.trim()
  if (!name) {
    createError.value = t('workspace.projectNameRequired')
    return
  }
  if (name.length > NAME_MAX) {
    createError.value = t('pcreate.nameTooLong', { n: NAME_MAX })
    return
  }
  const now = new Date().toISOString()
  try {
    const project = await api.saveProject({
      id: crypto.randomUUID(),
      name,
      description: newDesc.value.trim(),
      variables: {},
      created_at: now,
      updated_at: now,
    })
    open.value = false
    toast.success(t('workspace.projectCreated'), { message: name })
    emit('created', project)
  } catch (e) {
    toast.error(t('workspace.projectCreateFail'), { message: e instanceof Error ? e.message : String(e), duration: 6000 })
  }
}
</script>

<template>
  <Modal v-model:open="open" :title="t('projectlist.newProject')" width="420px" @close="open = false">
    <div class="form-field">
      <label class="form-label" for="new-project-name">{{ t('workspace.projectName') }}</label>
      <input
        id="new-project-name"
        v-model="newName"
        class="rf-input"
        :class="{ 'rf-input-error': createError }"
        :placeholder="t('workspace.projectNamePh')"
        maxlength="60"
        spellcheck="false"
        @input="createError = null"
        @keyup.enter="confirmCreate"
      />
      <p v-if="createError" class="rf-field-error" role="alert">{{ createError }}</p>
    </div>
    <div class="form-field">
      <label class="form-label" for="new-project-desc">{{ t('workspace.projectDesc') }}</label>
      <textarea
        id="new-project-desc"
        v-model="newDesc"
        class="rf-textarea"
        :maxlength="DESC_MAX"
        :placeholder="t('workspace.projectDescPh')"
        rows="3"
      ></textarea>
    </div>
    <template #footer>
      <button class="rf-btn" type="button" @click="open = false">{{ t('common.cancel') }}</button>
      <button class="rf-btn rf-btn-primary" type="button" :disabled="api.pending.value" @click="confirmCreate">
        {{ t('workspace.create') }}
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.form-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 12px;
}

.form-label {
  font-size: 12px;
  color: var(--text-2);
}

.rf-input-error {
  border-color: var(--danger) !important;
}
.rf-input-error:focus {
  border-color: var(--danger) !important;
  box-shadow: 0 0 0 2px var(--danger-tint) !important;
}
</style>
