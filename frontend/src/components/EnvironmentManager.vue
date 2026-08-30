<script setup lang="ts">
/**
 * EnvironmentManager：环境管理（大弹窗双栏）。
 *
 * 左侧 Sidebar：
 * - 「全局组」：全局变量 / 全局参数 / Vault Secrets（占位，后续版本启用）；
 * - 「环境组」：环境列表（色点 + 名称 + 默认模块基址），底部「+ 新建环境」。
 *
 * 右侧详情主面板：
 * - Header：环境名称编辑 + 摘要；
 * - 前置 URL 配置表：模块 (Module) | 前置 URL (Base URL) | 默认 | 操作 —— 在线编辑、增删；
 * - 环境变量表：变量名 | 远程值 | 本地值 | 启用 | 操作 —— 本地值优先覆盖远程值；
 * - 底部统一「保存 / 取消」变更控制，未保存切换环境 / 关闭需确认。
 *
 * 所有编辑作用于本地副本，保存时一次落库（store.updateEnvironment upsert）。
 */
import { computed, ref, watch } from 'vue'
import { useWorkspaceStore } from '../stores/workspace'
import { useToast } from '../composables/useToast'
import { defaultModule, envBaseUrl, envColorClass, normalizeBaseUrl } from '../utils/environment'
import Icon from './ui/Icon.vue'
import IconButton from './ui/IconButton.vue'
import Modal from './ui/Modal.vue'
import Popconfirm from './ui/Popconfirm.vue'
import type {
  Environment,
  EnvironmentVariable,
  GlobalParam,
  ModuleUrlConfig,
} from '../types/foxApi'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ 'update:open': [open: boolean] }>()

const store = useWorkspaceStore()
const toast = useToast()

const envs = ref<Environment[]>([])
const selected = ref<Environment | null>(null)
const busy = ref(false)
const dirty = ref(false)

/** 右侧详情面板作用域：'env' = 环境详情；'global' = 全局变量；'params' = 全局参数。 */
const scope = ref<'env' | 'global' | 'params'>('env')
/** 全局变量本地副本（作用域 global 时编辑此表，保存时整体落库）。 */
const globalVars = ref<EnvironmentVariable[]>([])
const globalDirty = ref(false)
/** 全局参数本地副本（作用域 params 时编辑此表，保存时整体落库）。 */
const globalParams = ref<GlobalParam[]>([])
const paramsDirty = ref(false)

const activeEnvId = computed(() => store.activeEnvId)

/**
 * 当前项目在该环境下的「实际默认模块」：项目绑定模块优先，其次兜底 is_default。
 * 运行时解析（地址栏前缀 / 发送 / {{base_url}}）与这里的展示一致。
 */
const effectiveDefaultId = computed<string | null>(
  () => (selected.value ? defaultModule(selected.value, store.project?.id)?.id ?? null : null),
)

/** 全局组：全局变量 / 全局参数已启用；Vault Secrets 仍为占位（置灰）。 */
const globalItems = [
  { key: 'global_variables', label: '全局变量', desc: '跨项目共享，{{name}} 按名引用（优先级最低）', enabled: true },
  { key: 'global_params', label: '全局参数', desc: '每个请求自动注入 query / header', enabled: true },
  { key: 'vault_secrets', label: 'Vault Secrets', desc: '密钥托管（规划中）', enabled: false },
]

function clone<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T
}

function ensureDefaultModule(env: Environment): void {
  if (env.modules.length > 0 && !env.modules.some((m) => m.is_default)) {
    env.modules[0].is_default = true
  }
}

function select(env: Environment | null): void {
  selected.value = env ? clone(env) : null
  if (selected.value) ensureDefaultModule(selected.value)
  dirty.value = false
  scope.value = 'env'
}

function selectGlobal(): void {
  scope.value = 'global'
  globalVars.value = clone(store.globalVariables)
  globalDirty.value = false
  selected.value = null
}

function selectParams(): void {
  scope.value = 'params'
  globalParams.value = clone(store.globalParams)
  paramsDirty.value = false
  selected.value = null
}

function addGlobalVariable(): void {
  globalVars.value.push({
    key: '',
    remote_value: '',
    local_value: '',
    enabled: true,
    description: null,
  })
  globalDirty.value = true
}

function removeGlobalVariable(index: number): void {
  globalVars.value.splice(index, 1)
  globalDirty.value = true
}

function onGlobalChange(): void {
  globalDirty.value = true
}

function addGlobalParam(): void {
  globalParams.value.push({ key: '', value: '', enabled: true, location: 'header' })
  paramsDirty.value = true
}

function removeGlobalParam(index: number): void {
  globalParams.value.splice(index, 1)
  paramsDirty.value = true
}

function onParamsChange(): void {
  paramsDirty.value = true
}

// ---------- 未保存修改的关闭/切换确认 ----------
const confirmLeave = ref(false)
let pendingAction: (() => void) | null = null

function hasPending(): boolean {
  return dirty.value || globalDirty.value || paramsDirty.value
}

function guardClose(): boolean {
  if (!hasPending()) return true
  confirmLeave.value = true
  return false
}

function guard(action: () => void): void {
  if (!hasPending()) {
    action()
    return
  }
  pendingAction = action
  confirmLeave.value = true
}

function discardChanges(): void {
  const act = pendingAction
  pendingAction = null
  confirmLeave.value = false
  dirty.value = false
  globalDirty.value = false
  paramsDirty.value = false
  act?.()
}

function keepEditing(): void {
  pendingAction = null
  confirmLeave.value = false
}

watch(
  () => props.open,
  (isOpen) => {
    if (!isOpen) return
    envs.value = clone(store.environments)
    const active = store.environments.find((e) => e.id === store.activeEnvId)
    select(active ?? store.environments[0] ?? null)
    globalVars.value = clone(store.globalVariables)
    globalDirty.value = false
    globalParams.value = clone(store.globalParams)
    paramsDirty.value = false
  },
)

function addEnvironment(): void {
  const now = new Date().toISOString()
  const env: Environment = {
    id: crypto.randomUUID(),
    name: '新环境',
    modules: [],
    variables: [],
    created_at: now,
    updated_at: now,
  }
  envs.value.push(env)
  select(env)
  dirty.value = true
}

// ---------- 模块（Module Base URLs） ----------
function addModule(): void {
  const env = selected.value
  if (!env) return
  const isFirst = env.modules.length === 0
  env.modules.push({
    id: crypto.randomUUID(),
    project_id: null,
    module_name: isFirst ? '默认' : '新模块',
    base_url: '',
    is_default: isFirst,
  })
  dirty.value = true
}

function removeModule(index: number): void {
  const env = selected.value
  if (!env) return
  env.modules.splice(index, 1)
  ensureDefaultModule(env)
  dirty.value = true
}

// ---------- 环境变量 ----------
function addVariable(): void {
  const env = selected.value
  if (!env) return
  env.variables.push({
    key: '',
    remote_value: '',
    local_value: '',
    enabled: true,
    description: null,
  })
  dirty.value = true
}

function removeVariable(index: number): void {
  const env = selected.value
  if (!env) return
  env.variables.splice(index, 1)
  dirty.value = true
}

function onAnyChange(): void {
  dirty.value = true
}

function variablesCount(): number {
  return selected.value?.variables.filter((v) => v.enabled).length ?? 0
}

async function save(): Promise<void> {
  if (busy.value) return
  if (scope.value === 'params') {
    busy.value = true
    try {
      const normalized: GlobalParam[] = globalParams.value
        .filter((p) => p.key.trim() !== '')
        .map((p) => ({ ...p, key: p.key.trim() }))
      await store.saveGlobalParams(normalized)
      globalParams.value = clone(store.globalParams)
      paramsDirty.value = false
      confirmLeave.value = false
      toast.success('全局参数已保存')
    } catch (err) {
      toast.error('保存全局参数失败', { message: err instanceof Error ? err.message : String(err) })
    } finally {
      busy.value = false
    }
    return
  }
  if (scope.value === 'global') {
    busy.value = true
    try {
      const normalized: EnvironmentVariable[] = globalVars.value
        .filter((v) => v.key.trim() !== '')
        .map((v) => ({ ...v, key: v.key.trim() }))
      await store.saveGlobalVariables(normalized)
      globalVars.value = clone(store.globalVariables)
      globalDirty.value = false
      confirmLeave.value = false
      toast.success('全局变量已保存')
    } catch (err) {
      toast.error('保存全局变量失败', { message: err instanceof Error ? err.message : String(err) })
    } finally {
      busy.value = false
    }
    return
  }
  if (!selected.value) return
  const name = selected.value.name.trim()
  if (!name) {
    toast.warning('环境名称不能为空')
    return
  }
  const env = selected.value
  ensureDefaultModule(env)
  const normalizedModules: ModuleUrlConfig[] = env.modules.map((m) => ({
    ...m,
    module_name: m.module_name.trim(),
    base_url: normalizeBaseUrl(m.base_url),
  }))
  const normalizedVariables: EnvironmentVariable[] = env.variables
    .filter((v) => v.key.trim() !== '')
    .map((v) => ({ ...v, key: v.key.trim() }))
  busy.value = true
  try {
    const saved = await store.updateEnvironment(
      {
        ...env,
        name,
        modules: normalizedModules,
        variables: normalizedVariables,
      },
      { silent: true },
    )
    selected.value = clone(saved)
    envs.value = clone(store.environments)
    dirty.value = false
    confirmLeave.value = false
    toast.success(`环境已保存：${saved.name}`)
  } catch (err) {
    toast.error('保存环境失败', { message: err instanceof Error ? err.message : String(err) })
  } finally {
    busy.value = false
  }
}

function cancel(): void {
  guard(() => {
    envs.value = clone(store.environments)
    const active = store.environments.find((e) => e.id === store.activeEnvId)
    select(active ?? store.environments[0] ?? null)
    globalVars.value = clone(store.globalVariables)
    globalDirty.value = false
    globalParams.value = clone(store.globalParams)
    paramsDirty.value = false
    emit('update:open', false)
  })
}

async function remove(env: Environment): Promise<void> {
  const persisted = store.environments.some((e) => e.id === env.id)
  try {
    if (persisted) await store.deleteEnvironment(env.id)
    const idx = envs.value.findIndex((e) => e.id === env.id)
    envs.value = envs.value.filter((e) => e.id !== env.id)
    if (selected.value?.id === env.id) {
      select(envs.value[Math.min(idx, Math.max(envs.value.length - 1, 0))] ?? null)
    }
    toast.success(`环境已删除：${env.name}`)
  } catch (err) {
    toast.error('删除环境失败', { message: err instanceof Error ? err.message : String(err) })
  }
}
</script>

<template>
  <Modal
    :open="open"
    title="环境管理"
    width="min(1120px, 94vw)"
    :guard-close="guardClose"
    @update:open="emit('update:open', $event)"
  >
    <div class="em">
      <div v-if="confirmLeave" class="em-confirm" role="alert">
        <span class="em-confirm-text">当前环境有未保存的修改</span>
        <span class="em-confirm-actions">
          <button class="rf-btn rf-btn-sm" type="button" @click="keepEditing">继续编辑</button>
          <button class="rf-btn rf-btn-sm rf-btn-danger" type="button" @click="discardChanges">
            放弃修改
          </button>
        </span>
      </div>
      <div class="em-body">
        <!-- ============ 左侧 Sidebar ============ -->
        <aside class="em-side">
          <div class="em-group">
            <div class="em-group-title">全局</div>
            <div
              v-for="item in globalItems"
              :key="item.key"
              class="em-global-row"
              :class="{
                active:
                  (item.key === 'global_variables' && scope === 'global') ||
                  (item.key === 'global_params' && scope === 'params'),
                disabled: !item.enabled,
              }"
              :title="
                item.enabled
                  ? `${item.label}：${item.desc}`
                  : `${item.label}：${item.desc}（规划中）`
              "
              @click="
                item.enabled &&
                  (item.key === 'global_params' ? guard(selectParams) : guard(selectGlobal))
              "
            >
              <span class="edot ed-global"></span>
              <span class="em-global-name">{{ item.label }}</span>
              <span v-if="!item.enabled" class="em-global-soon">soon</span>
              <span v-else-if="item.key === 'global_variables'" class="em-global-count">
                {{ store.globalVariables.filter((v) => v.enabled).length }}
              </span>
              <span v-else-if="item.key === 'global_params'" class="em-global-count">
                {{ store.globalParams.filter((p) => p.enabled).length }}
              </span>
            </div>
          </div>

          <div class="em-group em-group-envs">
            <div class="em-group-title">环境</div>
            <div class="em-list-body">
              <div
                v-for="env in envs"
                :key="env.id"
                class="em-row"
                :class="{ active: env.id === activeEnvId, sel: env.id === selected?.id }"
                @click="guard(() => select(env))"
              >
                <span class="edot" :class="`ed-${envColorClass(env.name)}`"></span>
                <span class="em-row-name" v-tooltip-overflow="env.name">{{ env.name }}</span>
                <span v-if="envBaseUrl(env, store.project?.id)" class="em-row-url">{{ envBaseUrl(env, store.project?.id) }}</span>
                <span v-if="env.id === activeEnvId" class="em-row-active">当前</span>
                <Popconfirm
                  :title="`删除环境「${env.name}」？删除后不可恢复。`"
                  confirm-text="删除"
                  @confirm="remove(env)"
                >
                  <IconButton name="trash" :size="12" tone="danger" class="em-row-del" />
                </Popconfirm>
              </div>
            </div>
            <button class="rf-btn rf-btn-sm em-add" type="button" @click="addEnvironment">
              <Icon name="plus" :size="13" /> 新建环境
            </button>
          </div>
        </aside>

        <!-- ============ 右侧详情 ============ -->
        <section class="em-editor">
          <template v-if="scope === 'env' && selected">
            <div class="em-editor-head">
              <input
                v-model="selected.name"
                class="rf-input em-name"
                placeholder="环境名称（必填）"
                spellcheck="false"
                @input="onAnyChange"
              />
              <span class="em-editor-meta">
                {{ selected.modules.length }} 模块 · {{ variablesCount() }} 变量生效
              </span>
            </div>

            <!-- 前置 URL 配置表 -->
            <div class="em-section">
              <div class="em-section-head">
                <span class="em-section-title">前置 URL（服务 / 模块）</span>
                <span class="em-section-hint">
                  项目模块随项目自动同步（只填基址）；未绑定模块的接口使用**所在项目**的模块基址
                </span>
              </div>
              <div class="em-table">
                <div class="em-th em-th-mod">
                  <span class="em-col-mod">模块</span>
                  <span class="em-col-base">前置 URL</span>
                  <span class="em-col-op"></span>
                </div>
                <div
                  v-for="(m, i) in selected.modules"
                  :key="m.id"
                  class="em-tr em-tr-mod"
                  :class="{ 'is-default': m.id === effectiveDefaultId }"
                >
                  <template v-if="m.project_id">
                    <span class="em-col-mod em-mod-project" :title="'项目模块：随项目「' + m.module_name + '」自动同步'">
                      <Icon name="folder" :size="12" class="em-mod-ic" />
                      <span class="em-mod-name" v-tooltip-overflow="m.module_name">{{ m.module_name }}</span>
                    <span
                      v-if="m.id === effectiveDefaultId"
                      class="em-mod-effective"
                      title="当前激活项目实际使用的默认基址（项目绑定模块优先）"
                    >本项目默认</span>
                    </span>
                  </template>
                  <template v-else>
                    <input
                      v-model="m.module_name"
                      class="rf-input rf-input-sm em-col-mod"
                      placeholder="如 支付 / 收单 / api"
                      spellcheck="false"
                      @input="onAnyChange"
                    />
                  </template>
                  <input
                    v-model="m.base_url"
                    class="rf-input rf-input-sm em-col-base"
                    placeholder="https://service.example.com（可含 {{变量}}）"
                    spellcheck="false"
                    @input="onAnyChange"
                  />
                  <IconButton
                    v-if="!m.project_id"
                    name="trash"
                    :size="13"
                    tone="danger"
                    title="删除模块"
                    class="em-col-op"
                    @click="removeModule(i)"
                  />

                </div>
                <button class="rf-btn rf-btn-sm em-add-var" type="button" @click="addModule">
                  <Icon name="plus" :size="13" /> 添加模块
                </button>
              </div>
            </div>

            <!-- 环境变量表 -->
            <div class="em-section">
              <div class="em-section-head">
                <span class="em-section-title">环境变量</span>
                <span class="em-section-hint">本地值优先覆盖远程值；停用不参与注入</span>
              </div>
              <div class="em-table">
                <div class="em-th em-th-var">
                  <span class="em-col-key">变量名</span>
                  <span class="em-col-remote">远程值</span>
                  <span class="em-col-local">本地值</span>
                  <span class="em-col-enabled">启用</span>
                  <span class="em-col-op"></span>
                </div>
                <div
                  v-for="(v, i) in selected.variables"
                  :key="i"
                  class="em-tr em-tr-var"
                  :class="{ off: !v.enabled }"
                >
                  <input
                    v-model="v.key"
                    class="rf-input rf-input-sm em-col-key"
                    placeholder="如 token"
                    spellcheck="false"
                    @input="onAnyChange"
                  />
                  <input
                    v-model="v.remote_value"
                    class="rf-input rf-input-sm em-col-remote"
                    placeholder="远程 / 公共值"
                    spellcheck="false"
                    @input="onAnyChange"
                  />
                  <input
                    v-model="v.local_value"
                    class="rf-input rf-input-sm em-col-local"
                    placeholder="本地覆盖值（可选）"
                    spellcheck="false"
                    @input="onAnyChange"
                  />
                  <input
                    v-model="v.enabled"
                    type="checkbox"
                    class="em-col-enabled"
                    :checked="v.enabled"
                    @change="onAnyChange"
                  />
                  <IconButton
                    name="trash"
                    :size="13"
                    tone="danger"
                    title="删除变量"
                    class="em-col-op"
                    @click="removeVariable(i)"
                  />
                </div>
                <button class="rf-btn rf-btn-sm em-add-var" type="button" @click="addVariable">
                  <Icon name="plus" :size="13" /> 添加变量
                </button>
              </div>
            </div>
          </template>

          <!-- 全局变量详情 -->
          <template v-else-if="scope === 'global'">
            <div class="em-editor-head">
              <span class="em-editor-title">全局变量</span>
              <span class="em-editor-meta">
                跨项目共享 · 优先级最低（环境 > 项目 > 全局） · {{ globalVars.filter((v) => v.enabled).length }} 个生效
              </span>
            </div>
            <div class="em-section">
              <div class="em-section-head">
                <span class="em-section-title">变量</span>
                <span class="em-section-hint">本地值优先覆盖远程值；停用不参与注入；&#123;&#123;变量&#125;&#125; 在请求中兜底可用</span>
              </div>
              <div class="em-table">
                <div class="em-th em-th-var">
                  <span class="em-col-key">变量名</span>
                  <span class="em-col-remote">远程值</span>
                  <span class="em-col-local">本地值</span>
                  <span class="em-col-enabled">启用</span>
                  <span class="em-col-op"></span>
                </div>
                <div
                  v-for="(v, i) in globalVars"
                  :key="i"
                  class="em-tr em-tr-var"
                  :class="{ off: !v.enabled }"
                >
                  <input
                    v-model="v.key"
                    class="rf-input rf-input-sm em-col-key"
                    placeholder="如 domain"
                    spellcheck="false"
                    @input="onGlobalChange"
                  />
                  <input
                    v-model="v.remote_value"
                    class="rf-input rf-input-sm em-col-remote"
                    placeholder="远程 / 公共值"
                    spellcheck="false"
                    @input="onGlobalChange"
                  />
                  <input
                    v-model="v.local_value"
                    class="rf-input rf-input-sm em-col-local"
                    placeholder="本地覆盖值（可选）"
                    spellcheck="false"
                    @input="onGlobalChange"
                  />
                  <input
                    v-model="v.enabled"
                    type="checkbox"
                    class="em-col-enabled"
                    :checked="v.enabled"
                    @change="onGlobalChange"
                  />
                  <IconButton
                    name="trash"
                    :size="13"
                    tone="danger"
                    title="删除变量"
                    class="em-col-op"
                    @click="removeGlobalVariable(i)"
                  />
                </div>
                <button
                  class="rf-btn rf-btn-sm em-add-var"
                  type="button"
                  @click="addGlobalVariable"
                >
                  <Icon name="plus" :size="13" /> 添加变量
                </button>
              </div>
            </div>
          </template>
          <!-- 全局参数详情 -->
          <template v-else-if="scope === 'params'">
            <div class="em-editor-head">
              <span class="em-editor-title">全局参数</span>
              <span class="em-editor-meta">
                每个请求自动注入 · 请求显式同名优先 · {{ globalParams.filter((p) => p.enabled).length }} 个生效
              </span>
            </div>
            <div class="em-section">
              <div class="em-section-head">
                <span class="em-section-title">参数</span>
                <span class="em-section-hint">query = 拼入 URL 查询参数；header = 注入请求头；值支持 &#123;&#123;变量&#125;&#125;</span>
              </div>
              <div class="em-table">
                <div class="em-th em-th-param">
                  <span class="em-col-key">参数名</span>
                  <span class="em-col-remote">值</span>
                  <span class="em-col-loc">位置</span>
                  <span class="em-col-enabled">启用</span>
                  <span class="em-col-op"></span>
                </div>
                <div
                  v-for="(p, i) in globalParams"
                  :key="i"
                  class="em-tr em-tr-param"
                  :class="{ off: !p.enabled }"
                >
                  <input
                    v-model="p.key"
                    class="rf-input rf-input-sm em-col-key"
                    placeholder="如 X-Request-Id"
                    spellcheck="false"
                    @input="onParamsChange"
                  />
                  <input
                    v-model="p.value"
                    class="rf-input rf-input-sm em-col-remote"
                    placeholder="值（可含 {{变量}}）"
                    spellcheck="false"
                    @input="onParamsChange"
                  />
                  <select
                    v-model="p.location"
                    class="rf-input rf-input-sm em-col-loc"
                    @change="onParamsChange"
                  >
                    <option value="header">Header</option>
                    <option value="query">Query</option>
                  </select>
                  <input
                    v-model="p.enabled"
                    type="checkbox"
                    class="em-col-enabled"
                    :checked="p.enabled"
                    @change="onParamsChange"
                  />
                  <IconButton
                    name="trash"
                    :size="13"
                    tone="danger"
                    title="删除参数"
                    class="em-col-op"
                    @click="removeGlobalParam(i)"
                  />
                </div>
                <button class="rf-btn rf-btn-sm em-add-var" type="button" @click="addGlobalParam">
                  <Icon name="plus" :size="13" /> 添加参数
                </button>
              </div>
            </div>
          </template>
          <p v-else class="em-empty">
            暂无环境。点击左侧「新建环境」创建，或从工作区顶部环境选择器进入。
          </p>
        </section>
      </div>
    </div>

    <template #footer>
      <button class="rf-btn" type="button" @click="cancel">取消</button>
      <button
        class="rf-btn rf-btn-primary"
        type="button"
        :disabled="busy || (!dirty && !globalDirty && !paramsDirty)"
        @click="save"
      >
        {{ busy ? '保存中…' : '保存' }}
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.em {
  display: flex;
  flex-direction: column;
  gap: 14px;
  min-height: 60vh;
  max-height: 70vh;
}

/* ---- 未保存修改确认条 ---- */
.em-confirm {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border: 1px solid var(--warning-tint, var(--border));
  border-radius: var(--radius);
  background: var(--warning-tint, var(--bg-panel));
}

.em-confirm-text {
  flex: 1;
  font-size: 12.5px;
  color: var(--warning);
  font-weight: 500;
}

.em-confirm-actions {
  display: inline-flex;
  gap: 8px;
  flex-shrink: 0;
}

/* ---- 双栏主区 ---- */
.em-body {
  display: flex;
  gap: 14px;
  min-height: 0;
  flex: 1;
}

/* ============ 左侧 Sidebar ============ */
.em-side {
  width: 240px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-panel);
  padding: 10px;
  overflow: hidden;
}

.em-group {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.em-group-envs {
  flex: 1;
  min-height: 0;
}

.em-group-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-3);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  padding: 4px 6px;
}

.em-global-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--radius);
  opacity: 0.6;
}

.em-global-row.active {
  background: var(--accent-tint);
  border: 1px solid rgba(124, 105, 245, 0.4);
  opacity: 1;
  cursor: pointer;
}

.em-global-row:not(.disabled) {
  opacity: 1;
  cursor: pointer;
  transition:
    background var(--dur) var(--ease),
    opacity var(--dur) var(--ease);
}

.em-global-row:not(.disabled):hover {
  background: var(--bg-hover);
}

.em-global-name {
  font-size: 12.5px;
  color: var(--text-2);
}

.em-global-count {
  margin-left: auto;
  font-size: 10px;
  color: var(--accent);
  background: var(--accent-tint);
  border-radius: 999px;
  padding: 0 6px;
  line-height: 1.6;
}

.em-global-name {
  font-size: 12.5px;
  color: var(--text-2);
}

.em-global-soon {
  margin-left: auto;
  font-size: 9px;
  letter-spacing: 0.04em;
  color: var(--text-3);
  border: 1px solid var(--border);
  border-radius: 999px;
  padding: 0 6px;
  line-height: 1.5;
}

.em-list-body {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.em-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--radius);
  border: 1px solid transparent;
  cursor: pointer;
  transition:
    background var(--dur) var(--ease),
    border-color var(--dur) var(--ease);
}

.em-row:hover {
  background: var(--bg-hover);
}

.em-row.sel {
  background: var(--bg-active);
}

.em-row.active {
  background: var(--accent-tint);
  border-color: rgba(124, 105, 245, 0.4);
}

.em-row-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--text-1);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.em-row-url {
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-3);
  max-width: 70px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex-shrink: 1;
}

.em-row-active {
  font-size: 10px;
  line-height: 1.6;
  color: var(--accent);
  background: var(--accent-tint);
  border-radius: 999px;
  padding: 0 6px;
  flex-shrink: 0;
}

.em-row-del {
  margin-left: auto;
  opacity: 0;
  width: 22px;
  height: 22px;
}

.em-row:hover .em-row-del,
.em-row.sel .em-row-del {
  opacity: 1;
}

.em-add {
  margin-top: 4px;
  width: 100%;
  border-style: dashed;
  color: var(--text-2);
}

/* ============ 右侧详情 ============ */
.em-editor {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 14px;
  overflow-y: auto;
}

.em-editor-head {
  display: flex;
  align-items: center;
  gap: 10px;
}

.em-name {
  flex: 1;
  height: var(--h-md);
}

.em-editor-title {
  flex: 1;
  font-size: 15px;
  font-weight: 600;
  color: var(--text-1);
}

.em-editor-meta {
  font-size: 11px;
  color: var(--text-3);
  flex-shrink: 0;
}

.em-section {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.em-section-head {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 0 2px;
}

.em-section-title {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-1);
}

.em-section-hint {
  font-size: 10.5px;
  color: var(--text-3);
}

.em-table {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-panel);
  padding: 6px;
}

.em-th {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 2px 8px 6px;
  font-size: 10.5px;
  font-weight: 600;
  color: var(--text-3);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.em-th-mod {
  margin-bottom: 4px;
}

.em-tr {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 0;
}

.em-tr.off {
  opacity: 0.45;
}

.em-tr-mod.is-default {
  background: var(--accent-tint);
  border-radius: var(--radius);
}

.em-tr-mod.is-default .em-col-mod {
  font-weight: 600;
}

/* 「本项目默认」徽标：项目绑定模块中当前项目实际生效的那一个 */
.em-mod-effective {
  flex-shrink: 0;
  margin-left: 6px;
  padding: 1px 7px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 600;
  white-space: nowrap;
  color: #34d399;
  background: rgba(52, 211, 153, 0.1);
}

/* 模块表列 */
.em-col-mod {
  width: 22%;
  min-width: 0;
}

.em-mod-project {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: var(--h-sm, 26px);
  padding: 0 8px;
  border: 1px dashed var(--border-strong);
  border-radius: var(--radius);
  color: var(--text-2);
  font-size: 12.5px;
}

.em-mod-ic {
  color: var(--accent);
  flex-shrink: 0;
}

.em-mod-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.em-col-base {
  flex: 1;
  min-width: 0;
  font-family: var(--font-mono);
}

/* 变量表列 */
.em-th-var {
  margin-bottom: 4px;
}

.em-th-param {
  margin-bottom: 4px;
}

.em-col-loc {
  width: 96px;
  flex-shrink: 0;
  font-size: 12px;
}

.em-col-key {
  width: 22%;
  min-width: 0;
}

.em-col-remote {
  flex: 1;
  min-width: 0;
  font-family: var(--font-mono);
}

.em-col-local {
  width: 24%;
  min-width: 0;
  font-family: var(--font-mono);
}

.em-col-enabled {
  width: 42px;
  flex-shrink: 0;
  accent-color: var(--accent);
  cursor: pointer;
}

.em-col-op {
  width: 24px;
  height: 24px;
  flex-shrink: 0;
}

.em-add-var {
  margin-top: 4px;
  border-style: dashed;
  color: var(--text-2);
}

.em-empty {
  margin: auto;
  font-size: 12.5px;
  color: var(--text-3);
  text-align: center;
}

/* ---- 环境色点 ---- */
.edot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
  background: var(--text-3);
}
.ed-dev {
  background: var(--success);
}
.ed-test {
  background: var(--info);
}
.ed-staging {
  background: var(--warning);
}
.ed-prod {
  background: #f97316;
}
.ed-global {
  background: var(--accent);
}
</style>
