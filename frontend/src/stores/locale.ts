/**
 * locale.ts：全局语言状态机（跟随系统 / 中文 / English），对标 theme.ts。
 *
 * - localeMode：用户偏好（system | zh | en），持久化到 localStorage；
 * - resolved：实际生效语言（zh | en），system 时按 navigator.language 推导；
 * - t(key, params?)：模板与 TS 文案统一入口，模板内调用随 resolved 自动重渲染；
 * - init：挂载前同步生效并写 <html lang>，避免首屏闪文案。
 */
import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { zh, type ZhKey } from '../i18n/zh'
import { en } from '../i18n/en'

export type LocaleMode = 'system' | 'zh' | 'en'
export type ResolvedLocale = 'zh' | 'en'
export type I18nKey = ZhKey
export type I18nParams = Record<string, string | number>

const STORAGE_KEY = 'rustfox.locale.mode'
const DEFAULT_MODE: LocaleMode = 'system'

function isMode(v: string | null): v is LocaleMode {
  return v === 'system' || v === 'zh' || v === 'en'
}

function readStored(): LocaleMode {
  try {
    const v = localStorage.getItem(STORAGE_KEY)
    return isMode(v) ? v : DEFAULT_MODE
  } catch {
    return DEFAULT_MODE
  }
}

function systemLocale(): ResolvedLocale {
  try {
    return (navigator.language || 'en').toLowerCase().startsWith('zh') ? 'zh' : 'en'
  } catch {
    return 'zh'
  }
}

function applyToDom(locale: ResolvedLocale): void {
  try {
    document.documentElement.setAttribute('lang', locale === 'zh' ? 'zh-CN' : 'en')
  } catch {
    // 非 DOM 环境（单测）忽略
  }
}

export function translate(locale: ResolvedLocale, key: string, params?: I18nParams): string {
  const dict = locale === 'zh' ? zh : en
  let text: string = (dict as Record<string, string>)[key] ?? (zh as Record<string, string>)[key] ?? key
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      text = text.split(`{${k}}`).join(String(v))
    }
  }
  return text
}

/**
 * 非组件环境的兜底翻译：供纯 utils（解析器抛错等）在任意调用点取当前语言文案；
 * 无活动 Pinia（部分纯函数单测）时回退中文，保证不崩。
 */
export function tFallback(key: string, params?: I18nParams): string {
  try {
    return useLocaleStore().t(key, params)
  } catch {
    return translate('zh', key, params)
  }
}

/**
 * 新建实体默认名（创建时按当前语言取值）与判定（中英双语都认，
 * 用户切语言后旧草稿仍能正确触发「命名确认」）。
 */
export type DefaultNameKind = 'endpoint' | 'project' | 'example' | 'folder'

const DEFAULT_NAME_KEYS: Record<DefaultNameKind, string> = {
  endpoint: 'default.endpointName',
  project: 'default.projectName',
  example: 'default.exampleName',
  folder: 'default.folderName',
}

export function defaultName(kind: DefaultNameKind, locale: ResolvedLocale): string {
  return translate(locale, DEFAULT_NAME_KEYS[kind])
}

export function isDefaultName(kind: DefaultNameKind, name: string): boolean {
  if (!name.trim()) return true
  return translate('zh', DEFAULT_NAME_KEYS[kind]) === name || translate('en', DEFAULT_NAME_KEYS[kind]) === name
}

export const useLocaleStore = defineStore('locale', () => {
  const mode = ref<LocaleMode>(readStored())

  /** 实际生效语言：system 跟随系统，否则取用户偏好。 */
  const resolved = computed<ResolvedLocale>(() =>
    mode.value === 'system' ? systemLocale() : mode.value,
  )

  /** 翻译函数（模板内调用，resolved 变化自动重渲染）。 */
  function t(key: I18nKey | string, params?: I18nParams): string {
    return translate(resolved.value, key, params)
  }

  function apply(): void {
    applyToDom(resolved.value)
  }

  /** 切换语言偏好并持久化，即时生效。 */
  function setMode(next: LocaleMode): void {
    mode.value = next
    try {
      localStorage.setItem(STORAGE_KEY, next)
    } catch {
      // 存储不可用：仅本次会话生效
    }
    apply()
  }

  /** 应用启动初始化：读取偏好 → 立即生效。 */
  function init(): void {
    apply()
  }

  return { mode, resolved, t, setMode, init, apply }
})
