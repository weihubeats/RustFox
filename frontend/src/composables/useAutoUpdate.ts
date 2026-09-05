/**
 * useAutoUpdate：定时检查更新（tauri-plugin-updater）。
 *
 * - 仅 Tauri 环境生效，浏览器预览静默跳过；
 * - 启动后延迟一次检查（避开启动关键路径），之后按间隔轮询；
 * - localStorage 节流：距上次实际检查不足间隔则跳过（多窗口/频繁重启不重复打扰）；
 * - 同一版本只提醒一次（关闭提醒后不再弹，直到出现更新的版本）；
 * - 发现新版时暂存 Update 对象，关于弹窗打开时直接承接（免二次检查，一键下载安装）；
 * - 自动检查失败静默忽略（不弹错，避免噪音；手动检查仍会报错）。
 */
import { check, type Update } from '@tauri-apps/plugin-updater'

export const AUTO_UPDATE_STARTUP_DELAY_MS = 8_000
export const AUTO_UPDATE_POLL_INTERVAL_MS = 6 * 60 * 60 * 1000

const LAST_CHECK_KEY = 'rustfox:update:last-check'
const NOTIFIED_VERSION_KEY = 'rustfox:update:notified-version'
const SKIPPED_VERSION_KEY = 'rustfox:update:skipped-version'

function readNumber(key: string): number {
  try {
    const v = Number(localStorage.getItem(key))
    return Number.isFinite(v) ? v : 0
  } catch {
    return 0
  }
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value)
  } catch {
    // 存储不可用时仅本次生效
  }
}

function readString(key: string): string {
  try {
    return localStorage.getItem(key) ?? ''
  } catch {
    return ''
  }
}

/** 跳过指定版本（不再提醒，直到出现更新的版本；可在设置中取消）。 */
export function skipUpdateVersion(version: string): void {
  if (version) write(SKIPPED_VERSION_KEY, version)
}

/** 当前跳过的版本（无则 null）。 */
export function skippedUpdateVersion(): string | null {
  const v = readString(SKIPPED_VERSION_KEY)
  return v || null
}

/** 取消跳过（下次检查到该版本会重新提醒）。 */
export function clearSkippedUpdateVersion(): void {
  try {
    localStorage.removeItem(SKIPPED_VERSION_KEY)
  } catch {
    // 忽略
  }
}
/** 待安装的更新（自动检查暂存，关于弹窗打开时取走接管）。 */
let pendingAutoUpdate: Update | null = null

export function takePendingUpdate(): Update | null {
  const u = pendingAutoUpdate
  pendingAutoUpdate = null
  return u
}

function storePendingUpdate(update: Update): void {
  pendingAutoUpdate?.close()
  pendingAutoUpdate = update
}

export interface AutoUpdateOptions {
  /** 检查函数（默认走 updater 插件；单测注入）。 */
  checkFn?: () => Promise<Update | null>
  /** 时间源（单测注入）。 */
  now?: () => number
  /** 启动延迟（默认 8s）。 */
  startupDelayMs?: number
  /** 轮询间隔（默认 6h，同时作为节流下限）。 */
  pollIntervalMs?: number
  /** 发现新版回调（App 层接 toast + 打开关于弹窗）。 */
  onUpdateAvailable?: (info: { version: string }) => void
}

/** 启动定时检查，返回停止函数（卸载时调用）。 */
export function startAutoUpdate(opts: AutoUpdateOptions = {}): () => void {
  const {
    checkFn = check,
    now = Date.now,
    startupDelayMs = AUTO_UPDATE_STARTUP_DELAY_MS,
    pollIntervalMs = AUTO_UPDATE_POLL_INTERVAL_MS,
    onUpdateAvailable,
  } = opts

  if (!('__TAURI_INTERNALS__' in window)) return () => undefined

  let stopped = false
  let interval: ReturnType<typeof setInterval> | undefined

  async function doCheck(): Promise<void> {
    if (stopped) return
    // 节流：距上次实际检查不足一个轮询间隔则跳过
    if (now() - readNumber(LAST_CHECK_KEY) < pollIntervalMs) return
    write(LAST_CHECK_KEY, String(now()))
    let update: Update | null = null
    try {
      update = await checkFn()
    } catch {
      return
    }
    if (stopped) {
      update?.close()
      return
    }
    if (!update?.available) {
      update?.close()
      return
    }
    const notified = readString(NOTIFIED_VERSION_KEY)
    if (notified === update.version) {
      update.close()
      return
    }
    // 用户跳过的版本不再提醒（出现更新的版本时恢复提醒）
    if (readString(SKIPPED_VERSION_KEY) === update.version) {
      update.close()
      return
    }
    write(NOTIFIED_VERSION_KEY, update.version)
    storePendingUpdate(update)
    onUpdateAvailable?.({ version: update.version })
  }

  const timer = setTimeout(() => {
    void doCheck()
    interval = setInterval(() => {
      void doCheck()
    }, pollIntervalMs)
  }, startupDelayMs)

  return () => {
    stopped = true
    clearTimeout(timer)
    if (interval) clearInterval(interval)
  }
}
