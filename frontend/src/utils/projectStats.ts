/**
 * projectStats：仪表板项目统计聚合（list_project_stats 载荷 → 计数 / 最近接口）。
 *
 * 输入为后端 snake_case 载荷（foxApi.d.ts ProjectStat）。曾因 Rust 侧
 * camelCase 序列化导致前端读取 undefined、统计显示异常——本模块对每条
 * 记录做显式字段校验：畸形条目（含 camelCase 键）直接跳过，宁显示 0
 * 不产出 NaN；Rust 侧由序列化契约测试锁死键名。
 */
import type { HttpMethod, ProjectStat } from '../types/foxApi'

export interface LatestEndpoint {
  method: HttpMethod
  path: string
}

export interface AggregatedProjectStats {
  counts: Record<string, number>
  latest: Record<string, LatestEndpoint | null>
}

/** 统计聚合：按 project_id 汇总接口数与最近更新的接口；缺字段的条目跳过。 */
export function aggregateProjectStats(stats: ProjectStat[] | null | undefined): AggregatedProjectStats {
  const counts: Record<string, number> = {}
  const latest: Record<string, LatestEndpoint | null> = {}
  for (const s of stats ?? []) {
    if (typeof s?.project_id !== 'string' || typeof s.endpoint_count !== 'number') continue
    counts[s.project_id] = s.endpoint_count
    latest[s.project_id] =
      typeof s.latest_method === 'string' && typeof s.latest_path === 'string'
        ? { method: s.latest_method as HttpMethod, path: s.latest_path }
        : null
  }
  return { counts, latest }
}

/** 全部项目接口总数（无有效条目时为 0，不会出现 NaN）。 */
export function totalEndpointCount(counts: Record<string, number>): number {
  return Object.values(counts).reduce((a, b) => a + b, 0)
}
