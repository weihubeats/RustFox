<script setup lang="ts">
/**
 * AuthPanel：认证配置面板（none/bearer/basic/apikey/oauth2/digest/hawk/awsv4/hmac/dynamic_signature）。
 * OAuth2 授权流：后端起本地回调 + 打开系统浏览器，完成后令牌写入草稿。
 * 签名类（Hawk/AWS SigV4/HMAC/Digest/动态签名）：由后端实时计算注入请求头。
 */
import { computed, ref } from 'vue'
import { useFoxApi } from '../composables/useFoxApi'
import { useToast } from '../composables/useToast'
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

const AUTH_TYPES: Array<{ value: string; label: string }> = [
  { value: 'none', label: '无认证' },
  { value: 'bearer', label: 'Bearer Token' },
  { value: 'basic', label: 'Basic' },
  { value: 'apikey', label: 'API Key' },
  { value: 'dynamic_signature', label: '动态签名' },
  { value: 'oauth2', label: 'OAuth2' },
  { value: 'digest', label: 'Digest' },
  { value: 'hawk', label: 'Hawk' },
  { value: 'awsv4', label: 'AWS Signature V4' },
  { value: 'hmac', label: 'HMAC (AK-SK)' },
]
const AUTH_IN_OPTIONS = [
  { value: 'header', label: 'Header' },
  { value: 'query', label: 'Query' },
]
const ALGORITHM_OPTIONS: Array<{ value: SignatureAlgorithm; label: string }> = [
  { value: 'md5', label: 'MD5' },
  { value: 'sha256', label: 'SHA-256' },
  { value: 'hmac_sha256', label: 'HMAC-SHA256' },
]
const ENCODING_OPTIONS: Array<{ value: SignatureEncoding; label: string }> = [
  { value: 'hex_lower', label: 'Hex（小写）' },
  { value: 'hex_upper', label: 'Hex（大写）' },
  { value: 'base64', label: 'Base64' },
]

/** 载荷模板占位符提示文案（含字面 `{{ }}`，避免在模板文本里嵌套花括号）。 */
const SIG_HINT =
  '占位符 {{$key}} / {{$secret}} / {{$timestamp}}。时间戳在 Rust 后端发送前最后一刻生成（不经 IPC 往返），保证实时性；Key / Timestamp / Sig 三个请求头自动注入。'

/** 签名类认证的发送时行为说明。 */
const SIGN_HINTS: Record<string, string> = {
  digest: '发送时先不带凭据，收到 401 质询后自动应答重发（MD5 / SHA-256，qop=auth）。',
  hawk: '发送时用时间戳 + 随机数实时计算 Hawk mac，有 Body 时附带 payload hash。',
  awsv4: '发送时按区域 + 服务名做 SigV4 规范签名（x-amz-date + Authorization）。',
  hmac: '发送时附带 X-Access-Key / X-Timestamp / X-Nonce / X-Signature 四个头。',
}

/** 当前认证类型的行为说明（签名类才有）。 */
const signHint = computed(() => {
  const t = authAny.value?.type
  return (t && SIGN_HINTS[t]) || ''
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

const sigConfig = computed(() => authAny.value?.config as EditableSignatureConfig)

/** OAuth2 授权状态文案。 */
const oauthStatus = computed(() => {
  const token = authAny.value?.token as
    | { access_token?: string; expires_at?: string }
    | undefined
  if (!token?.access_token) return '未授权'
  const expires = token.expires_at ? new Date(token.expires_at) : null
  const expiring = expires ? expires.getTime() - Date.now() < 5 * 60_000 : false
  return expires && !expiring
    ? `已授权，有效期至 ${expires.toLocaleString('zh-CN')}`
    : expiring
      ? '令牌即将过期，发送时将自动刷新'
      : '已授权（发送时自动刷新）'
})

/** 发起完整授权流：后端起本地回调 + 打开系统浏览器；完成后令牌写入草稿。 */
async function oauthAuthorize(): Promise<void> {
  if (!props.draft) return
  authorizing.value = true
  try {
    const token = await api.oauthAuthorize(authAny.value as AuthSpec)
    const req = props.draft.request
    req.auth = { ...authAny.value, token } as AuthSpec
    toast.success('OAuth2 授权成功，请保存 (⌘S) 持久化')
  } catch (err) {
    toast.error('OAuth2 授权失败', { message: err instanceof Error ? err.message : String(err) })
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
      toast.warning('App-Secret 仅用于签名计算，不会明文发送或存储', {
        message:
          'Key/Timestamp/Sig 三头由后端发送前实时生成；请勿把 Secret 值粘贴到 Header 或载荷模板之外。',
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
        placeholder="用户名"
      />
      <input
        v-model="authAny.password"
        class="rf-input rf-input-sm kv-value"
        placeholder="密码"
        type="password"
      />
    </div>
    <div v-else-if="authAny?.type === 'oauth2'" class="oauth-form">
      <p class="oauth-hint">
        <span class="oauth-status" :class="{ ok: oauthStatus !== '未授权' }">{{ oauthStatus }}</span>
        <button
          class="rf-btn rf-btn-sm"
          type="button"
          :disabled="authorizing"
          @click="oauthAuthorize"
        >
          {{ authorizing ? '授权中…' : '立即授权' }}
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
        <input v-model="authAny.scope" class="rf-input rf-input-sm kv-key" placeholder="Scope（空格分隔）" />
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
        placeholder="用户名"
      />
      <input
        v-model="authAny.password"
        class="rf-input rf-input-sm kv-value"
        placeholder="密码"
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
        <input v-model="authAny.region" class="rf-input rf-input-sm kv-key" placeholder="Region（如 us-east-1）" spellcheck="false" />
        <input v-model="authAny.service" class="rf-input rf-input-sm kv-value" placeholder="Service（如 iam / s3）" spellcheck="false" />
      </div>
      <div class="kv-row">
        <input v-model="authAny.session_token" class="rf-input rf-input-sm kv-value" placeholder="Session Token（临时凭证选填）" spellcheck="false" />
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
          {{ advancedOpen ? '收起高级配置' : '高级配置 ▾' }}
        </button>
        <div v-if="advancedOpen" class="sig-adv-body">
          <div class="kv-row">
            <input
              v-model="sigConfig.key_header"
              class="rf-input rf-input-sm kv-key"
              placeholder="Key 头名"
              spellcheck="false"
            />
            <input
              v-model="sigConfig.timestamp_header"
              class="rf-input rf-input-sm kv-key"
              placeholder="时间戳头名"
              spellcheck="false"
            />
            <input
              v-model="sigConfig.sig_header"
              class="rf-input rf-input-sm kv-value"
              placeholder="签名头名"
              spellcheck="false"
            />
          </div>
          <div class="kv-row">
            <span class="sig-label">算法</span>
            <CustomSelect
              :model-value="sigConfig.algorithm"
              :options="ALGORITHM_OPTIONS"
              size="sm"
              class="sig-select"
              @update:model-value="sigConfig.algorithm = String($event)"
            />
            <span class="sig-label">编码</span>
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