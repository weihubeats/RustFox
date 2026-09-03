<script setup lang="ts">
/**
 * RealtimeView：实时调试视图（WebSocket / SSE）。
 *
 * - WebSocket：后端 `fox-http::ws_client` 早已完备（自动重连/心跳/离线补发），
 *   本视图经新增的 ws_* 命令接入：建连 → 收发帧 → 断开，事件经 `fox:ws-event` 推送；
 * - SSE：`sse_connect` 拉流转发原始文本块，前端按帧解析（event/data/id），事件经
 *   `fox:sse-event` 推送；断线续传带 `Last-Event-ID`。
 * - 消息日志上限 500 条（超限丢弃最旧）；卸载视图时自动断开，避免后台泄漏。
 */
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useFoxApi } from '../composables/useFoxApi'
import { useToast } from '../composables/useToast'
import Icon from '../components/ui/Icon.vue'
import Tabs, { type TabItem } from '../components/ui/Tabs.vue'
import type { SseEventPayload, WsEventPayload } from '../types/foxApi'

const router = useRouter()
const api = useFoxApi()
const toast = useToast()

/**
 * 多窗口：实时视图可在独立窗口打开（边工作边监控 WS/SSE）。
 * 工作区本身保持单窗口——双开工作区会导致两份 Pinia 草稿分叉，
 * 故仅实时视图（无本地草稿态）支持弹出。
 */
const isMainWindow = ref(true)
async function detectWindow(): Promise<void> {
  try {
    if (!('__TAURI_INTERNALS__' in window)) return
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    isMainWindow.value = getCurrentWindow().label === 'main'
  } catch {
    // 非 Tauri 环境：保持默认（主窗口行为）
  }
}

async function popout(): Promise<void> {
  try {
    // Tauri v2：带 URL 的新窗口经 WebviewWindow 创建。
    const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
    const label = `realtime-${Date.now()}`
    const win = new WebviewWindow(label, {
      url: '/realtime',
      title: 'RustFox · 实时调试',
      width: 960,
      height: 700,
      minWidth: 640,
      minHeight: 480,
    })
    await win.once('tauri://error', (e: { payload: unknown }) => {
      toast.error('新窗口打开失败', { message: String(e.payload) })
    })
  } catch (err) {
    toast.error('新窗口打开失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

type MainTab = 'ws' | 'sse'
const mainTab = ref<MainTab>('ws')
const MAIN_TABS: TabItem[] = [
  { key: 'ws', label: 'WebSocket' },
  { key: 'sse', label: 'SSE' },
]

const MAX_LOG = 500
function cap<T>(arr: T[]): T[] {
  if (arr.length > MAX_LOG) arr.splice(0, arr.length - MAX_LOG)
  return arr
}
function nowTime(): string {
  return new Date().toLocaleTimeString('zh-CN', { hour12: false })
}

// ---------- WebSocket ----------
const wsUrl = ref('ws://127.0.0.1:4010')
const wsAutoReconnect = ref(true)
const wsConnId = ref<string | null>(null)
const wsState = ref<'idle' | 'connecting' | 'open' | 'closed' | 'error'>('idle')
const wsConnecting = ref(false)

interface WsLogItem {
  t: string
  dir: 'in' | 'out' | 'sys'
  frame: string
  text: string
}
const wsLog = ref<WsLogItem[]>([])

function wsPush(dir: WsLogItem['dir'], frame: string, text: string): void {
  wsLog.value.push({ t: nowTime(), dir, frame, text })
  cap(wsLog.value)
}

async function wsConnect(): Promise<void> {
  if (!wsUrl.value.trim() || wsConnecting.value || wsConnId.value) return
  wsConnecting.value = true
  try {
    const id = await api.wsConnect({
      url: wsUrl.value.trim(),
      auto_reconnect: wsAutoReconnect.value,
    })
    wsConnId.value = id
    wsState.value = 'connecting'
    wsPush('sys', 'sys', `正在连接 ${wsUrl.value.trim()}…`)
  } catch (err) {
    toast.error('WS 连接失败', { message: err instanceof Error ? err.message : String(err) })
  } finally {
    wsConnecting.value = false
  }
}

async function wsDisconnect(silent = false): Promise<void> {
  if (!wsConnId.value) return
  const id = wsConnId.value
  wsConnId.value = null
  try {
    await api.wsDisconnect(id)
  } catch {
    // 连接可能已失效：本地状态照常清理
  }
  wsState.value = 'closed'
  if (!silent) wsPush('sys', 'sys', '已断开连接')
}

const wsSendText = ref('')
async function wsSend(frame: 'text' | 'ping'): Promise<void> {
  if (!wsConnId.value || !wsSendText.value) return
  const payload = wsSendText.value
  try {
    await api.wsSend({ connection_id: wsConnId.value, frame, payload })
    wsPush('out', frame, payload)
    wsSendText.value = ''
  } catch (err) {
    toast.error('发送失败', { message: err instanceof Error ? err.message : String(err) })
  }
}

function onWsEvent(payload: WsEventPayload): void {
  if (payload.connection_id !== wsConnId.value) return
  if (payload.kind === 'state') {
    const s = payload.state
    wsState.value =
      s === 'open' ? 'open' : s === 'connecting' ? 'connecting' : s === 'closed' ? 'closed' : 'error'
    wsPush('sys', 'sys', `状态：${s}`)
  } else if (payload.kind === 'message') {
    wsPush('in', payload.frame, payload.text)
  } else {
    wsState.value = 'error'
    wsPush('sys', 'sys', `失败：${payload.message}`)
  }
}

const wsStateText = computed(() => {
  switch (wsState.value) {
    case 'open':
      return '已连接'
    case 'connecting':
      return '连接中…'
    case 'closed':
      return '已断开'
    case 'error':
      return '异常'
    default:
      return '未连接'
  }
})

// ---------- SSE ----------
const sseUrl = ref('http://127.0.0.1:4010/sse')
const sseConnId = ref<string | null>(null)
const sseStatus = ref<'idle' | 'open' | 'closed' | 'error'>('idle')
const sseConnecting = ref(false)
const sseBuffer = ref('')
const sseLastId = ref('')

interface SseLogItem {
  t: string
  event: string
  data: string
  id: string
}
const sseLog = ref<SseLogItem[]>([])

/** SSE 帧解析：按空行切帧，data 行换行拼接，id 行更新续传位点。 */
function parseSseFrames(): void {
  const normalized = sseBuffer.value.replace(/\r\n/g, '\n')
  const parts = normalized.split('\n\n')
  sseBuffer.value = parts.pop() ?? ''
  for (const frame of parts) {
    const dataLines: string[] = []
    let event = 'message'
    let id = ''
    for (const line of frame.split('\n')) {
      if (!line || line.startsWith(':')) continue
      const colon = line.indexOf(':')
      const field = (colon === -1 ? line : line.slice(0, colon)).trim()
      const value = (colon === -1 ? '' : line.slice(colon + 1)).replace(/^ /, '')
      if (field === 'event') event = value || 'message'
      else if (field === 'data') dataLines.push(value)
      else if (field === 'id') id = value
    }
    if (dataLines.length === 0) continue
    if (id) sseLastId.value = id
    sseLog.value.push({ t: nowTime(), event, data: dataLines.join('\n'), id })
    cap(sseLog.value)
  }
}

async function sseConnect(): Promise<void> {
  if (!sseUrl.value.trim() || sseConnecting.value || sseConnId.value) return
  sseConnecting.value = true
  try {
    const id = await api.sseConnect({
      url: sseUrl.value.trim(),
      last_event_id: sseLastId.value || null,
    })
    sseConnId.value = id
    sseStatus.value = 'open'
  } catch (err) {
    toast.error('SSE 订阅失败', { message: err instanceof Error ? err.message : String(err) })
  } finally {
    sseConnecting.value = false
  }
}

async function sseDisconnect(silent = false): Promise<void> {
  if (!sseConnId.value) return
  const id = sseConnId.value
  sseConnId.value = null
  try {
    await api.sseDisconnect(id)
  } catch {
    // 忽略：本地状态照常清理
  }
  sseStatus.value = 'closed'
  if (!silent) {
    sseLog.value.push({ t: nowTime(), event: 'sys', data: '已取消订阅', id: '' })
    cap(sseLog.value)
  }
}

function onSseEvent(payload: SseEventPayload): void {
  if (payload.connection_id !== sseConnId.value) return
  if (payload.kind === 'open') {
    sseStatus.value = 'open'
    sseLog.value.push({ t: nowTime(), event: 'sys', data: '订阅已建立', id: '' })
  } else if (payload.kind === 'chunk') {
    sseBuffer.value += payload.chunk
    // 单块超 1MB 未成帧：截断防内存膨胀（畸形流保护）。
    if (sseBuffer.value.length > 1_000_000) sseBuffer.value = sseBuffer.value.slice(-200_000)
    parseSseFrames()
  } else if (payload.kind === 'error') {
    sseStatus.value = 'error'
    sseLog.value.push({ t: nowTime(), event: 'sys', data: `错误：${payload.message}`, id: '' })
  } else {
    sseStatus.value = 'closed'
    sseConnId.value = null
    sseLog.value.push({ t: nowTime(), event: 'sys', data: '服务端关闭了流', id: '' })
  }
  cap(sseLog.value)
}

// ---------- 事件订阅 ----------
let unlistenWs: UnlistenFn | null = null
let unlistenSse: UnlistenFn | null = null

onMounted(async () => {
  void detectWindow()
  try {
    if ('__TAURI_INTERNALS__' in window) {
      unlistenWs = await listen<WsEventPayload>('fox:ws-event', (e) => onWsEvent(e.payload))
      unlistenSse = await listen<SseEventPayload>('fox:sse-event', (e) => onSseEvent(e.payload))
    }
  } catch {
    // 非 Tauri（浏览器预览）环境：忽略
  }
})

onUnmounted(() => {
  unlistenWs?.()
  unlistenSse?.()
  // 视图卸载即断开：后台会话随之释放，避免泄漏。
  void wsDisconnect(true)
  void sseDisconnect(true)
})
</script>

<template>
  <div class="rt-root">
    <div class="row rf-mb-2 rt-top">
      <button
        v-if="isMainWindow"
        class="rf-btn rf-btn-sm"
        type="button"
        @click="router.push('/workspace')"
      >
        ← 返回工作区
      </button>
      <button
        v-if="isMainWindow"
        class="rf-btn rf-btn-sm rf-btn-ghost"
        type="button"
        title="在独立窗口打开（边工作边监控）"
        @click="popout"
      >
        新窗口打开
      </button>
      <Tabs v-model="mainTab" :tabs="MAIN_TABS" size="sm" />
      <span class="rt-status" :class="mainTab === 'ws' ? `st-${wsState}` : `st-${sseStatus}`">
        {{
          mainTab === 'ws'
            ? wsStateText
            : sseStatus === 'open'
              ? '订阅中'
              : sseStatus === 'closed'
                ? '已结束'
                : sseStatus === 'error'
                  ? '异常'
                  : '未订阅'
        }}
      </span>
    </div>

    <!-- WebSocket -->
    <div v-if="mainTab === 'ws'">
      <div class="row rf-mb-2">
        <input
          v-model="wsUrl"
          class="rf-input rt-url"
          placeholder="ws://127.0.0.1:4010/socket"
          spellcheck="false"
          :disabled="!!wsConnId"
        />
        <label class="rt-check">
          <input v-model="wsAutoReconnect" type="checkbox" :disabled="!!wsConnId" />
          自动重连
        </label>
        <button
          v-if="!wsConnId"
          class="rf-btn rf-btn-sm rf-btn-primary"
          type="button"
          :disabled="wsConnecting || !wsUrl.trim()"
          @click="wsConnect"
        >
          {{ wsConnecting ? '连接中…' : '连接' }}
        </button>
        <button v-else class="rf-btn rf-btn-sm rf-btn-danger" type="button" @click="wsDisconnect()">
          断开
        </button>
        <button class="rf-btn rf-btn-sm rf-btn-ghost" type="button" @click="wsLog = []">清空日志</button>
      </div>

      <div class="rt-log">
        <div v-if="!wsLog.length" class="rt-empty">连接后在此查看收发的帧（文本 / 二进制 base64 / ping）</div>
        <div v-for="(m, i) in wsLog" :key="i" class="rt-line" :class="`dir-${m.dir}`">
          <span class="rt-time">{{ m.t }}</span>
          <span class="rt-dir">{{ m.dir === 'in' ? '↓' : m.dir === 'out' ? '↑' : '•' }}</span>
          <span class="rt-frame">{{ m.frame }}</span>
          <span class="rt-text">{{ m.text }}</span>
        </div>
      </div>

      <div class="row rf-mt-2">
        <input
          v-model="wsSendText"
          class="rf-input rt-url"
          placeholder="输入要发送的文本帧…"
          spellcheck="false"
          :disabled="!wsConnId || wsState !== 'open'"
          @keydown.enter="wsSend('text')"
        />
        <button
          class="rf-btn rf-btn-sm rf-btn-primary"
          type="button"
          :disabled="!wsConnId || wsState !== 'open' || !wsSendText"
          @click="wsSend('text')"
        >
          <Icon name="send" :size="13" /> 发送
        </button>
        <button
          class="rf-btn rf-btn-sm rf-btn-ghost"
          type="button"
          :disabled="!wsConnId || wsState !== 'open' || !wsSendText"
          title="以 Ping 帧发送输入内容"
          @click="wsSend('ping')"
        >
          Ping
        </button>
      </div>
    </div>

    <!-- SSE -->
    <div v-else>
      <div class="row rf-mb-2">
        <input
          v-model="sseUrl"
          class="rf-input rt-url"
          placeholder="http://127.0.0.1:4010/events"
          spellcheck="false"
          :disabled="!!sseConnId"
        />
        <button
          v-if="!sseConnId"
          class="rf-btn rf-btn-sm rf-btn-primary"
          type="button"
          :disabled="sseConnecting || !sseUrl.trim()"
          @click="sseConnect"
        >
          {{ sseConnecting ? '订阅中…' : '订阅' }}
        </button>
        <button v-else class="rf-btn rf-btn-sm rf-btn-danger" type="button" @click="sseDisconnect()">
          取消订阅
        </button>
        <button class="rf-btn rf-btn-sm rf-btn-ghost" type="button" @click="sseLog = []">清空日志</button>
        <span v-if="sseLastId" class="hint-inline">续传位点：{{ sseLastId }}</span>
      </div>

      <div class="rt-log">
        <div v-if="!sseLog.length" class="rt-empty">订阅后在此查看事件流（event / data / id）</div>
        <div v-for="(m, i) in sseLog" :key="i" class="rt-line dir-in">
          <span class="rt-time">{{ m.t }}</span>
          <span class="rt-frame">{{ m.event }}</span>
          <span v-if="m.id" class="rt-id">#{{ m.id }}</span>
          <span class="rt-text">{{ m.data }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.rt-root {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 12px 16px;
  gap: 4px;
  min-height: 0;
}
.row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.rt-top {
  justify-content: flex-start;
}
.rt-url {
  flex: 1;
  font-family: var(--font-mono);
  font-size: 12.5px;
}
.rt-check {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--text-2);
  white-space: nowrap;
}
.rt-status {
  font-size: 12px;
  font-weight: 600;
  padding: 2px 10px;
  border-radius: 999px;
  background: var(--bg-hover);
  color: var(--text-3);
}
.rt-status.st-open {
  color: var(--success);
  background: var(--success-tint);
}
.rt-status.st-connecting {
  color: var(--warning);
  background: var(--warning-tint);
}
.rt-status.st-error {
  color: var(--danger);
  background: var(--danger-tint);
}
.rt-log {
  flex: 1;
  min-height: 200px;
  overflow-y: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-card);
  padding: 8px 0;
  font-family: var(--font-mono);
  font-size: 12px;
}
.rt-empty {
  padding: 14px 16px;
  color: var(--text-3);
  font-family: var(--font-sans, inherit);
}
.rt-line {
  display: flex;
  gap: 8px;
  padding: 3px 12px;
  align-items: baseline;
  border-bottom: 1px dashed transparent;
}
.rt-line:hover {
  background: var(--bg-hover);
}
.rt-time {
  flex-shrink: 0;
  color: var(--text-3);
}
.rt-dir {
  flex-shrink: 0;
  width: 14px;
  text-align: center;
  font-weight: 700;
}
.dir-in .rt-dir {
  color: var(--success);
}
.dir-out .rt-dir {
  color: var(--info);
}
.dir-sys .rt-dir {
  color: var(--text-3);
}
.rt-frame {
  flex-shrink: 0;
  font-size: 10.5px;
  font-weight: 700;
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--bg-hover);
  color: var(--text-2);
}
.rt-id {
  flex-shrink: 0;
  color: var(--text-3);
}
.rt-text {
  flex: 1;
  min-width: 0;
  white-space: pre-wrap;
  word-break: break-all;
  color: var(--text-1);
}
.hint-inline {
  font-size: 11.5px;
  color: var(--text-3);
  white-space: nowrap;
}
</style>
