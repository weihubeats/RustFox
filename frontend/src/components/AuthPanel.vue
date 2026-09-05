<script setup lang="ts">
/**
 * AuthPanel：认证配置面板（none/bearer/basic/apikey/oauth2/digest/hawk/awsv4/hmac/dynamic_signature）。
 * OAuth2 授权流：后端起本地回调 + 打开系统浏览器，完成后令牌写入草稿。
 * 签名类（Hawk/AWS SigV4/HMAC/Digest/动态签名）：由后端实时计算注入请求头。
 */
import { computed, ref } from 'vue'
import { useFoxApi } from '../composables/useFoxApi'
import { useToast } from '../composables/useToast'
import { useLocaleStore } from '../stores/locale'
import CustomSelect from './ui/CustomSelect.vue'
import type {
  ApiKeyLocation,
  AuthSpec,
  DynamicSignatureConfig,
  Endpoint,
  OAuth2Token,
  SignatureAlgorithm,
  SignatureEncoding,
} from '../types/foxApi'

const props = defineProps<{ draft: Endpoint | null }>()

const api = useFoxApi()
const toast = useToast()
const locale = useLocaleStore()
const t = locale.t

const AUTH_TYPES = computed<Array<{ value: string; label: string }>>(() => [
  { value: 'none', label: t('auth.none') },
  { value: 'bearer', label: 'Bearer Token' },
  { value: 'basic', label: 'Basic' },
  { value: 'apikey', label: 'API Key' },
  { value: 'dynamic_signature', label: t('auth.dynamicSignature') },
  { value: 'oauth2', label: 'OAuth2' },
  { value: 'digest', label: 'Digest' },
  { value: 'hawk', label: 'Hawk' },
  { value: 'awsv4', label: 'AWS Signature V4' },
  { value: 'hmac', label: 'HMAC (AK-SK)' },
])
const AUTH_IN_OPTIONS = [
  { value: 'header', label: 'Header' },
  { value: 'query', label: 'Query' },
]
const ALGORITHM_OPTIONS: Array<{ value: SignatureAlgorithm; label: string }> = [
  { value: 'md5', label: 'MD5' },
  { value: 'sha256', label: 'SHA-256' },
  { value: 'hmac_sha256', label: 'HMAC-SHA256' },
]
const ENCODING_OPTIONS = computed<Array<{ value: SignatureEncoding; label: string }>>(() => [
  { value: 'hex_lower', label: t('auth.hexLower') },
  { value: 'hex_upper', label: t('auth.hexUpper') },
  { value: 'base64', label: 'Base64' },
])

/** 载荷模板占位符提示文案（含字面 `{{ }}`，避免在模板文本里嵌套花括号）。 */
const SIG_HINT = computed(() => t('auth.sigHint'))

/** 签名类认证的发送时行为说明。 */
const SIGN_HINTS = computed<Record<string, string>>(() => ({
  digest: t('auth.hintDigest'),
  hawk: t('auth.hintHawk'),
  awsv4: t('auth.hintAwsv4'),
  hmac: t('auth.hintHmac'),
}))

/** 当前认证类型的行为说明（签名类才有）。 */
const signHint = computed(() => {
  const type = authAny.value?.type
  return (type && SIGN_HINTS.value[type]) || ''
})

/** Auth 编辑区；type 切换时替换为对应默认对象。所有分支字段统一为可选项。 */
type EditableAuth = AuthSpec & {
  token?: OAuth2Token
  key?: string
  value?: string
  in?: ApiKeyLocation | string
  username?: string
  password?: string
  client_id?: string
  client_secret?: string
  auth_url?: string
  token_url?: string
  scope?: string
  redirect_uri?: string
  key_id?: string
  access_key?: string
  secret_key?: string
  region?: string
  service?: string
  session_token?: string
  /** 动态签名配置。algorithm / encoding 放宽为 string，配合 CustomSelect
   * 的 `string | number` 事件类型；提交时由后端按枚举校验。 */
  config?: DynamicSignatureConfig & { algorithm?: string; encoding?: string }

}

const authAny = computed(() => props.draft?.request.auth as EditableAuth)
const authorizing = ref(false)
const advancedOpen = ref(false)

/** 动态签名高级配置面板是否可展开（仅该类型下展示）。 */
const isSignature = computed(() => authAny.value?.type === 'dynamic_signature')

/** 动态签名配置引用。仅 `dynamic_signature` 分支渲染时访问（由模板
 * `v-else-if` 守卫保证 config 必存在），故此处做非空断言，模板内
 * 避免对可空 `config` 的链式访问。
 * algorithm / encoding 放宽为 string：CustomSelect 事件类型为
 * `string | number`，提交时后端按枚举反序列化校验。 */
type EditableSignatureConfig = Omit<DynamicSignatureConfig, 'algorithm' | 'encoding'> & {
  algorithm?: string
  encoding?: string
}

/** 动态签名配置引用。缺失时物化默认值（历史数据 / IPC 异常可能缺 config，
 * 直接访问字段会渲染崩溃），保证 sig-form 分支渲染不崩。 */
const sigConfig = computed<EditableSignatureConfig>(() => {
  const auth = authAny.value as EditableAuth & { config?: DynamicSignatureConfig } | undefined
  const cfg = auth?.config
  if (cfg) return cfg as EditableSignatureConfig
  const req = props.draft?.request
  if (req && auth?.type === 'dynamic_signature') {
    const created = defaultSignatureConfig()
    req.auth = { type: 'dynamic_signature', config: created } as AuthSpec
    return created as EditableSignatureConfig
  }
  return defaultSignatureConfig()
})

/** OAuth2 授权状态文案。 */
const oauthStatus = computed(() => {
  const token = authAny.value?.token as
    | { access_token?: string; expires_at?: string }
    | undefined
  if (!token?.access_token) return t('auth.unauthorized')
  const expires = token.expires_at ? new Date(token.expires_at) : null
  const expiring = expires ? expires.getTime() - Date.now() < 5 * 60_000 : false
  return expires && !expiring
    ? t('auth.authorizedUntil', { v: expires.toLocaleString(locale.resolved === 'zh' ? 'zh-CN' : 'en-US') })
    : expiring
      ? t('auth.expiringSoon')
      : t('auth.authorizedAuto')
})

/** 发起完整授权流：后端起本地回调 + 打开系统浏览器；完成后令牌写入草稿。 */
async function oauthAuthorize(): Promise<void> {
  if (!props.draft) return
  authorizing.value = true
  try {
    const token = await api.oauthAuthorize(authAny.value as AuthSpec)
    const req = props.draft.request
    req.auth = { ...authAny.value, token } as AuthSpec
    toast.success(t('auth.oauthOk'))
  } catch (err) {
    toast.error(t('auth.oauthFail'), { message: err instanceof Error ? err.message : String(err) })
  } finally {
    authorizing.value = false
  }
}

function defaultSignatureConfig(): DynamicSignatureConfig {
  return {
    app_key: '',
    app_secret: '',
    key_header: 'App-Key',
    timestamp_header: 'App-Timestamp',
    sig_header: 'App-Sig',
    algorithm: 'md5',
    encoding: 'hex_lower',
    payload_template: '{{$key}}{{$secret}}{{$timestamp}}',
  }
}

function setAuthType(type: string): void {
  const req = props.draft?.request
  if (!req) return
  switch (type) {
    case 'none':
      req.auth = { type: 'none' }
      break
    case 'bearer':
      req.auth = { type: 'bearer', token: '' }
      break
    case 'basic':
      req.auth = { type: 'basic', username: '', password: '' }
      break
    case 'apikey':
      req.auth = { type: 'apikey', key: '', value: '', in: 'header' }
      break
    case 'dynamic_signature':
      req.auth = { type: 'dynamic_signature', config: defaultSignatureConfig() }
      advancedOpen.value = true
      // 安全警告：Secret 只参与签名计算，绝不明文入请求头 / 不落明文库。
      toast.warning(t('auth.secretWarn'), {
        message: t('auth.secretWarnHint'),
        duration: 6000,
      })
      break
    case 'oauth2':
      req.auth = {
        type: 'oauth2',
        client_id: '',
        client_secret: '',
        auth_url: '',
        token_url: '',
        scope: '',
        redirect_uri: '',
      }
      break
    case 'digest':
      req.auth = { type: 'digest', username: '', password: '' }
      break
    case 'hawk':
      req.auth = { type: 'hawk', key_id: '', key: '' }
      break
    case 'awsv4':
      req.auth = {
        type: 'awsv4',
        access_key: '',
        secret_key: '',
        region: '',
        service: '',
        session_token: '',
      }
      break
    case 'hmac':
      req.auth = { type: 'hmac', access_key: '', secret_key: '' }
      break
  }
}
</script>

<template>
  <div class="panel">
    <CustomSelect
      :model-value="authAny?.type ?? 'none'"
      :options="AUTH_TYPES"
      size="sm"
      class="auth-type-select"
      @update:model-value="setAuthType(String($event))"
    />
    <div v-if="authAny?.type === 'bearer'" class="kv-row">
      <input
        v-model="authAny.token"
        class="rf-input rf-input-sm kv-value"
        placeholder="Token"
        spellcheck="false"
      />
    </div>
    <div v-else-if="authAny?.type === 'basic'" class="kv-row">
      <input
        v-model="authAny.username"
        class="rf-input rf-input-sm kv-key"
        :placeholder="t('auth.username')"
      />
      <input
        v-model="authAny.password"
        class="rf-input rf-input-sm kv-value"
        :placeholder="t('auth.password')"
        type="password"
      />
    </div>
    <div v-else-if="authAny?.type === 'oauth2'" class="oauth-form">
      <p class="oauth-hint">
        <span class="oauth-status" :class="{ ok: oauthStatus !== t('auth.unauthorized') }">{{ oauthStatus }}</span>
        <button
          class="rf-btn rf-btn-sm"
          type="button"
          :disabled="authorizing"
          @click="oauthAuthorize"
        >
          {{ authorizing ? t('auth.authorizing') : t('auth.authorizeNow') }}
        </button>
      </p>
      <div class="kv-row">
        <input v-model="authAny.client_id" class="rf-input rf-input-sm kv-key" placeholder="Client ID" />
        <input v-model="authAny.client_secret" class="rf-input rf-input-sm kv-value" placeholder="Client Secret" type="password" />
      </div>
      <div class="kv-row">
        <input v-model="authAny.auth_url" class="rf-input rf-input-sm kv-key" placeholder="Authorize URL" />
        <input v-model="authAny.token_url" class="rf-input rf-input-sm kv-value" placeholder="Token URL" />
      </div>
      <div class="kv-row">
        <input v-model="authAny.scope" class="rf-input rf-input-sm kv-key" :placeholder="t('auth.scopePh')" />
        <input v-model="authAny.redirect_uri" class="rf-input rf-input-sm kv-value" placeholder="Redirect URI" />
      </div>
    </div>
    <div v-else-if="authAny?.type === 'apikey'" class="kv-row">
      <input v-model="authAny.key" class="rf-input rf-input-sm kv-key" placeholder="Key" />
      <input
        v-model="authAny.value"
        class="rf-input rf-input-sm kv-value"
        placeholder="Value"
        spellcheck="false"
      />
      <CustomSelect v-model="authAny.in" :options="AUTH_IN_OPTIONS" size="sm" class="auth-in-select" />
    </div>
    <div v-else-if="authAny?.type === 'digest'" class="kv-row">
      <input
        v-model="authAny.username"
        class="rf-input rf-input-sm kv-key"
        :placeholder="t('auth.username')"
      />
      <input
        v-model="authAny.password"
        class="rf-input rf-input-sm kv-value"
        :placeholder="t('auth.password')"
        type="password"
      />
    </div>
    <div v-else-if="authAny?.type === 'hawk'" class="kv-row">
      <input v-model="authAny.key_id" class="rf-input rf-input-sm kv-key" placeholder="Key ID" spellcheck="false" />
      <input
        v-model="authAny.key"
        class="rf-input rf-input-sm kv-value"
        placeholder="Key"
        type="password"
        spellcheck="false"
      />
    </div>
    <div v-else-if="authAny?.type === 'awsv4'" class="sign-form">
      <div class="kv-row">
        <input v-model="authAny.access_key" class="rf-input rf-input-sm kv-key" placeholder="Access Key" spellcheck="false" />
        <input v-model="authAny.secret_key" class="rf-input rf-input-sm kv-value" placeholder="Secret Key" type="password" spellcheck="false" />
      </div>
      <div class="kv-row">
        <input v-model="authAny.region" class="rf-input rf-input-sm kv-key" :placeholder="t('auth.regionPh')" spellcheck="false" />
        <input v-model="authAny.service" class="rf-input rf-input-sm kv-value" :placeholder="t('auth.servicePh')" spellcheck="false" />
      </div>
      <div class="kv-row">
        <input v-model="authAny.session_token" class="rf-input rf-input-sm kv-value" :placeholder="t('auth.sessionTokenPh')" spellcheck="false" />
      </div>
    </div>
    <div v-else-if="authAny?.type === 'hmac'" class="kv-row">
      <input v-model="authAny.access_key" class="rf-input rf-input-sm kv-key" placeholder="Access Key" spellcheck="false" />
      <input
        v-model="authAny.secret_key"
        class="rf-input rf-input-sm kv-value"
        placeholder="Secret Key"
        type="password"
        spellcheck="false"
      />
    </div>
    <p v-if="signHint" class="auth-hint">
      {{ signHint }}
    </p>
    <div v-else-if="isSignature" class="sig-form">
      <div class="kv-row">
        <input
          v-model="sigConfig.app_key"
          class="rf-input rf-input-sm kv-key"
          placeholder="App Key"
          spellcheck="false"
        />
        <input
          v-model="sigConfig.app_secret"
          class="rf-input rf-input-sm kv-value"
          placeholder="App Secret"
          type="password"
          spellcheck="false"
        />
      </div>
      <div class="sig-adv">
        <button class="rf-btn rf-btn-sm sig-toggle" type="button" @click="advancedOpen = !advancedOpen">
          {{ advancedOpen ? t('auth.collapseAdv') : t('auth.expandAdv') }}
        </button>
        <div v-if="advancedOpen" class="sig-adv-body">
          <div class="kv-row">
            <input
              v-model="sigConfig.key_header"
              class="rf-input rf-input-sm kv-key"
              :placeholder="t('auth.keyHeaderPh')"
              spellcheck="false"
            />
            <input
              v-model="sigConfig.timestamp_header"
              class="rf-input rf-input-sm kv-key"
              :placeholder="t('auth.tsHeaderPh')"
              spellcheck="false"
            />
            <input
              v-model="sigConfig.sig_header"
              class="rf-input rf-input-sm kv-value"
              :placeholder="t('auth.sigHeaderPh')"
              spellcheck="false"
            />
          </div>
          <div class="kv-row">
            <span class="sig-label">{{ t('auth.algorithm') }}</span>
            <CustomSelect
              :model-value="sigConfig.algorithm"
              :options="ALGORITHM_OPTIONS"
              size="sm"
              class="sig-select"
              @update:model-value="sigConfig.algorithm = String($event)"
            />
            <span class="sig-label">{{ t('auth.encoding') }}</span>
            <CustomSelect
              :model-value="sigConfig.encoding"
              :options="ENCODING_OPTIONS"
              size="sm"
              class="sig-select"
              @update:model-value="sigConfig.encoding = String($event)"
            />
          </div>
          <textarea
            v-model="sigConfig.payload_template"
            class="rf-input rf-input-sm sig-template"
            placeholder="{{$key}}{{$secret}}{{$timestamp}}"
            spellcheck="false"
          ></textarea>
          <p class="sig-hint">
            {{ SIG_HINT }}
          </p>
        </div>
      </div>
    </div>

  </div>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.kv-row {
  display: flex;
  gap: 6px;
  align-items: center;
}

.kv-key {
  width: 220px;
}

.kv-value {
  flex: 1;
}

.auth-type-select {
  width: 160px;
}

.auth-in-select {
  width: 100px;
}

.oauth-hint {
  margin: 0;
  display: flex;
  align-items: center;
  gap: 10px;
}

.oauth-status {
  font-size: 12px;
  color: var(--text-3);
}

.oauth-status.ok {
  color: var(--success);
}

.auth-hint {
  margin: 0;
  font-size: 11.5px;
  color: var(--text-3);
}

.sign-form,
.sig-form {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.sig-adv {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.sig-toggle {
  align-self: flex-start;
  color: var(--text-3);
}

.sig-adv-body {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px;
  border: 1px solid var(--border);
  border-radius: 6px;
}

.sig-label {
  font-size: 12px;
  color: var(--text-3);
  white-space: nowrap;
}

.sig-select {
  width: 130px;
}

.sig-template {
  width: 100%;
  font-family: var(--font-mono, monospace);
  font-size: 12px;
  min-height: 40px;
  resize: vertical;
}

.sig-hint {
  margin: 0;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-3);
}

</style>