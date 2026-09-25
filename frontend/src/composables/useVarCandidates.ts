/**
 * useVarCandidates：{{变量}} 自动补全的候选名列表。
 *
 * 数据源与 EndpointEditor 地址栏同一张合并表（优先级 全局 < 项目 < 环境），
 * 外加后端 resolve_variables 支持的内置变量（$uuid / $timestamp / …）。
 * 返回 ComputedRef：环境 / 变量增删后候选即时刷新（随语言刷新的常量
 * 同理必须是 computed，见 AGENTS.md i18n 规范）。
 */
import { computed, type ComputedRef } from 'vue'
import { useWorkspaceStore } from '../stores/workspace'
import { environmentVariableMap, variableListToMap } from '../utils/environment'

/** fox-core variable.rs 的内置变量（{{$seq:key}} 形态不做候选，属模板写法）。 */
export const BUILTIN_VARIABLES = ['$uuid', '$timestamp', '$isoTimestamp', '$randomInt', '$seq'] as const

export function useVarCandidates(): ComputedRef<string[]> {
  const store = useWorkspaceStore()
  return computed(() => {
    const activeEnv = store.environments.find((e) => e.id === store.activeEnvId) ?? null
    const merged = {
      ...variableListToMap(store.globalVariables),
      ...(store.project?.variables ?? {}),
      ...environmentVariableMap(activeEnv, store.project?.id),
    }
    const set = new Set<string>(BUILTIN_VARIABLES)
    for (const key of Object.keys(merged)) set.add(key)
    return [...set]
  })
}
