/**
 * theme.ts：全局主题状态机（跟随系统 / 深色 / 浅色）。
 *
 * - themeMode：用户偏好（system | dark | light），持久化到 localStorage；
 * - resolved：实际生效主题（dark | light），由偏好 + 系统 `prefers-color-scheme` 推导；
 * - apply：把生效主题写入 <html> 的 `data-theme` 属性（全局 CSS 令牌唯一事实源），
 *   并广播 `rustfox:theme` 全局事件通知 JSON 编辑器等组件切换主题；
 * - init：应用启动时同步读取持久化偏好并立即生效，避免首屏闪白/闪黑（FOUC）。
 */
import { computed, ref } from 'vue'
import { defineStore } from 'pinia'

export type ThemeMode = 'system' | 'dark' | 'light'
export type ResolvedTheme = 'dark' | 'light'

const STORAGE_KEY = 'rustfox.theme.mode'
const DEFAULT_MODE: ThemeMode = 'dark'

/** 全局主题切换事件：detail.mode 为实际生效主题（CodeMirror 等编辑器监听）。 */
export const THEME_EVENT = 'rustfox:theme'

function isMode(v: string | null): v is ThemeMode {
  return v === 'system' || v === 'dark' || v === 'light'
}

function readStored(): ThemeMode {
  try {
    const v = localStorage.getItem(STORAGE_KEY)
    return isMode(v) ? v : DEFAULT_MODE
  } catch {
    return DEFAULT_MODE
  }
}

function systemPrefersDark(): boolean {
  try {
    return window.matchMedia('(prefers-color-scheme: dark)').matches
  } catch {
    return true
  }
}

/** 把生效主题写到 <html>（CSS 令牌以 data-theme 为准）并广播编辑器事件。 */
function applyToDom(mode: ResolvedTheme): void {
  const root = document.documentElement
  root.setAttribute('data-theme', mode)
  root.classList.toggle('dark', mode === 'dark')
  root.classList.toggle('light', mode === 'light')
  window.dispatchEvent(new CustomEvent<{ mode: ResolvedTheme }>(THEME_EVENT, { detail: { mode } }))
}

export const useThemeStore = defineStore('theme', () => {
  const mode = ref<ThemeMode>(readStored())
  const systemDark = ref(systemPrefersDark())

  /** 实际生效主题：system 跟随系统，否则取用户偏好。 */
  const resolved = computed<ResolvedTheme>(() =>
    mode.value === 'system' ? (systemDark.value ? 'dark' : 'light') : mode.value,
  )

  let media: MediaQueryList | null = null
  let onSystemChange: ((e: MediaQueryListEvent) => void) | null = null

  /** 应用当前生效主题到 DOM。 */
  function apply(): void {
    applyToDom(resolved.value)
  }

  /** 系统偏好变化 → 同步 systemDark，仅 system 模式需要实时重刷。 */
  function syncSystemListener(): void {
    media?.removeEventListener('change', onSystemChange!)
    media = null
    onSystemChange = null
    if (mode.value !== 'system') return
    media = window.matchMedia('(prefers-color-scheme: dark)')
    onSystemChange = (e) => {
      systemDark.value = e.matches
      apply()
    }
    media.addEventListener('change', onSystemChange)
  }

  /** 切换用户偏好并持久化。 */
  function setMode(next: ThemeMode): void {
    mode.value = next
    try {
      localStorage.setItem(STORAGE_KEY, next)
    } catch {
      // 存储不可用（隐私模式等）：仅本次会话生效
    }
    syncSystemListener()
    apply()
  }

  /** 应用启动初始化：读取偏好 → 立即生效 → 挂系统监听（防闪烁）。 */
  function init(): void {
    systemDark.value = systemPrefersDark()
    syncSystemListener()
    apply()
  }

  return { mode, resolved, setMode, init, apply }
})
