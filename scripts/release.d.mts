/** scripts/release.mjs 的类型声明（供 frontend 内 vitest / vue-tsc 消费）。 */
export interface ConsistencyResult {
  ok: boolean
  base: string
  versions: Record<string, string>
}

export function isValidSemver(v: string): boolean
export function resolveTargetVersion(current: string, arg: string): string
export function readJsonVersion(file: string): string
export function applyCargoVersion(content: string, version: string): string
export function syncVersions(root: string, target: string): string[]
export function checkConsistency(root: string): ConsistencyResult
export const VERSION_FILES: { conf: string; pkg: string; cargo: string }
