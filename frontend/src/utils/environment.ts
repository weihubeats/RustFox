/**
 * environment.ts：环境相关的前端工具（多模块 Base URL / 结构化变量 / URL 拼接）。
 *
 * - envColorClass：按环境名称启发式归类颜色（开发绿 / 测试蓝 / 预发布琥珀 / 生产橙 / 全局紫）；
 * - defaultModule / moduleBaseUrl / envBaseUrl：模块基址解析（未指定模块 → 默认模块）；
 * - environmentVariableMap：结构化变量 → 扁平 `{{name}}` 注入表（enabled、本地值优先）；
 * - resolveRequestUrl：请求拼接核心 —— 最终 URL = 选中模块前置 URL + 相对路径，缺省降级默认模块；
 * - resolveVariables：镜像 fox-core variable.rs 的 `{{name}}` 单层递归解析（深度上限 10）。
 */

import type { Environment, ModuleUrlConfig } from '../types/foxApi'

/** 环境名称 → 颜色类（映射类名，颜色值由调用方 scoped CSS 定义）。 */
export function envColorClass(name: string): string {
  const n = name.trim().toLowerCase()
  if (/(开发|development|dev)/.test(n)) return 'dev'
  if (/(测试|test|qa)/.test(n)) return 'test'
  if (/(预发布|staging|stage|pre)/.test(n)) return 'staging'
  if (/(生产|prod|production|live)/.test(n)) return 'prod'
  if (/(全局|global)/.test(n)) return 'global'
  return ''
}

/**
 * 默认模块：优先**当前项目绑定的模块**（project_id 匹配），其次 `is_default` 标记，
 * 否则取第一个（兼容无标记数据）。
 *
 * 项目偏好让多项目共用一个环境时，「默认模块」随所在项目自动落在该
 * 项目自己的基址上（开放演示 → jsonplaceholder，用户服务 → 本地 4010），
 * 而不是全局钉死在 is_default 的模块上。
 */
export function defaultModule(
  env: Environment | null | undefined,
  projectId?: string | null,
): ModuleUrlConfig | undefined {
  const modules = env?.modules ?? []
  if (projectId) {
    const own = modules.find((m) => m.project_id === projectId)
    if (own) return own
  }
  return modules.find((m) => m.is_default) ?? modules[0]
}

/** 按模块 id 或模块名解析模块；`key` 为空时取默认模块。 */
export function moduleByName(
  env: Environment | null | undefined,
  key: string | null | undefined,
  projectId?: string | null,
): ModuleUrlConfig | undefined {
  if (!env) return undefined
  const n = key?.trim()
  if (!n) return defaultModule(env, projectId)
  return (
    env.modules.find((m) => m.id === n || m.module_name === n) ?? defaultModule(env, projectId)
  )
}

/** 模块前置 URL（未指定 → 默认模块）；解析前的原始文本，保留 `{{变量}}`。 */
export function moduleBaseUrl(
  env: Environment | null | undefined,
  key?: string | null,
  projectId?: string | null,
): string {
  return moduleByName(env, key, projectId)?.base_url?.trim() ?? ''
}

/** 环境的「主 baseUrl」：默认模块基址；无模块时回退到名为 base_url 的已启用变量。 */
export function envBaseUrl(env: Environment | null | undefined, projectId?: string | null): string {
  if (!env) return ''
  const fromModule = defaultModule(env, projectId)?.base_url?.trim()
  if (fromModule) return fromModule
  const varRow = env.variables.find((v) => v.key === 'base_url' && v.enabled)
  return varRow ? effectiveVariable(varRow) : ''
}

/** 变量生效值：本地覆盖值非空时优先，否则取远程 / 公共值。 */
export function effectiveVariable(v: { remote_value: string; local_value: string }): string {
  return v.local_value.trim() || v.remote_value.trim()
}

/** 结构化变量列表 → 扁平注入表（enabled 才注入、本地值优先、本地覆盖生效）。 */
export function variableListToMap(
  vars: { key: string; remote_value: string; local_value: string; enabled: boolean }[] | null | undefined,
): Record<string, string> {
  const out: Record<string, string> = {}
  for (const v of vars ?? []) {
    if (!v.enabled) continue
    const key = v.key.trim()
    if (!key || key.startsWith('{{') || key.startsWith('$')) continue
    const value = effectiveVariable(v)
    if (value) out[key] = value
  }
  return out
}

/**
 * 结构化变量 → 扁平注入表（enabled 才注入；本地值优先；未知键原样）。
 * 默认模块基址自动以 `base_url` 注入（已有同名变量时不覆盖），
 * 使 `{{base_url}}` 在旧语义下继续可用。
 */
export function environmentVariableMap(
  env: Environment | null | undefined,
  projectId?: string | null,
): Record<string, string> {
  const out = variableListToMap(env?.variables)
  const base = envBaseUrl(env, projectId)
  if (base && !out.base_url) out.base_url = base
  return out
}

/** 规范化基础 URL：去掉尾部斜杠，避免与路径拼接出双斜杠（`https://x.com//posts`）。 */
export function normalizeBaseUrl(value: string): string {
  const s = value.trim()
  const stripped = s.replace(/\/+$/, '')
  // 保留协议完整性（避免 `https://` 被削成 `https:`）
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:\/+$/.test(s)) return s
  return stripped
}

/** 变量递归解析（镜像后端：单次扫描 + 深度上限；未知变量原样保留）。 */
export function resolveVariables(
  input: string,
  vars: Record<string, string>,
  depth = 0,
): string {
  if (depth >= 10) return input
  return input.replace(/\{\{\s*([^{}]+?)\s*\}\}/g, (full, name: string) => {
    const value = vars[name]
    if (value == null || value === '') return full
    return resolveVariables(value, vars, depth + 1)
  })
}

function isAbsolute(s: string): boolean {
  return s.startsWith('http://') || s.startsWith('https://')
}

/** 基址 + 相对路径拼接（与 fox-core util::build_url 一致：完整 URL 直用，否则去斜杠后拼接）。 */
export function joinBaseUrl(base: string, path: string): string {
  if (isAbsolute(path)) return path
  const p = path.trim().replace(/^\/+/, '')
  const b = normalizeBaseUrl(base)
  if (!b) return `/${p}`
  return `${b}/${p}`
}

export interface ResolvedRequestUrl {
  /** 最终请求 URL。 */
  url: string
  /** 实际命中的模块（id 或名称，缺省为默认模块名；无模块时为空串）。 */
  moduleName: string
  /** 未找到显式模块键的 fallback 标记（显式键无效时已回退默认模块）。 */
  fellBack: boolean
}

/**
 * 请求拼接核心（多模块匹配逻辑）：
 * 1. `path` 为完整 http(s) 地址 → 直接使用；
 * 2. 显式指定模块（id / 名称）→ 取该模块前置 URL；无效键回退默认模块；
 * 3. 未指定模块 → 默认模块；无任何模块 → 无基址，仅返回路径。
 * 基址 / 路径中的 `{{变量}}` 以「环境变量 + 调用方变量」合并表解析。
 */
export function resolveRequestUrl(
  env: Environment | null | undefined,
  moduleKey: string | null | undefined,
  path: string,
  extraVars: Record<string, string> = {},
  projectId?: string | null,
): ResolvedRequestUrl {
  const vars = { ...extraVars, ...environmentVariableMap(env, projectId) }
  const rendered = resolveVariables(path, vars)
  if (isAbsolute(rendered)) return { url: rendered, moduleName: '', fellBack: false }

  let target: ModuleUrlConfig | undefined
  let fellBack = false
  if (moduleKey?.trim()) {
    target = env?.modules.find((m) => m.id === moduleKey.trim() || m.module_name === moduleKey.trim())
    if (!target) {
      target = defaultModule(env, projectId)
      fellBack = !!target
    }
  } else {
    target = defaultModule(env, projectId)
  }

  const baseRaw = target?.base_url?.trim() ?? ''
  const base = resolveVariables(baseRaw, vars)
  return {
    url: joinBaseUrl(base, rendered),
    moduleName: target?.module_name ?? '',
    fellBack,
  }
}