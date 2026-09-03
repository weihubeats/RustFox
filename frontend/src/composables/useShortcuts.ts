/**
 * useShortcuts：全局快捷键集中注册表。
 *
 * 背景：快捷键原来散落在各组件的 window keydown 监听里（EndpointEditor、
 * ResponsePanel…），无统一展示、无冲突检查。集中注册后：
 * - 同一按键只响应最后注册（栈顶优先），卸载自动注销，无泄漏；
 * - 快捷键帮助面板直接由注册表生成，新增快捷键自动出现在帮助里；
 * - 输入框内（INPUT/TEXTAREA/contentEditable）默认不触发全局动作，
 *   需要的动作显式声明 `inInput: true`。
 */
import { onBeforeUnmount } from 'vue'

export interface ShortcutDef {
  /** 唯一标识（重复注册时后者覆盖前者）。 */
  id: string
  /** 修饰键（ctrl/meta 二选一或都不要；macOS 上 ctrl 视为 ⌘）。 */
  mod?: 'ctrl' | 'meta' | 'none'
  shift?: boolean
  alt?: boolean
  /** 主键（event.key，不区分大小写比较）。 */
  key: string
  /** 展示分组（帮助面板用）。 */
  group: string
  /** 展示文案。 */
  description: string
  /** 输入框内是否触发（默认 false）。 */
  inInput?: boolean
  handler: (event: KeyboardEvent) => void
}

const registry = new Map<string, ShortcutDef>()
let listening = false

function isEditable(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null
  if (!el || typeof (el as HTMLElement).tagName !== 'string') return false
  const tag = (el as HTMLElement).tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || (el as HTMLElement).isContentEditable
}

function modOk(def: ShortcutDef, e: KeyboardEvent): boolean {
  const mod = def.mod ?? 'ctrl'
  if (mod === 'none') return !e.ctrlKey && !e.metaKey
  // macOS 上 Ctrl 与 ⌘ 互通：任一按下即视为修饰符满足。
  return e.ctrlKey || e.metaKey
}

function onKeydown(e: KeyboardEvent): void {
  for (const def of [...registry.values()].reverse()) {
    if (!modOk(def, e)) continue
    if (!!def.shift !== e.shiftKey) continue
    if (!!def.alt !== e.altKey) continue
    if (def.key.toLowerCase() !== e.key.toLowerCase()) continue
    if (!def.inInput && isEditable(e.target)) continue
    e.preventDefault()
    def.handler(e)
    return
  }
}

function ensureListening(): void {
  if (listening) return
  listening = true
  window.addEventListener('keydown', onKeydown)
}

/** 注册全局快捷键（组件卸载时自动注销；同 id 后注册覆盖）。 */
export function registerShortcut(def: ShortcutDef): () => void {
  ensureListening()
  registry.set(def.id, def)
  return () => {
    if (registry.get(def.id) === def) registry.delete(def.id)
  }
}

/** 快捷键帮助面板的数据源（按 group 分组，保持注册顺序）。 */
export function shortcutGroups(): Array<{ group: string; items: ShortcutDef[] }> {
  const groups = new Map<string, ShortcutDef[]>()
  for (const def of registry.values()) {
    const list = groups.get(def.group)
    if (list) list.push(def)
    else groups.set(def.group, [def])
  }
  return [...groups.entries()].map(([group, items]) => ({ group, items }))
}

/** 展示用按键文案（⌘/Ctrl + Shift + Key）。 */
export function shortcutLabel(def: ShortcutDef): string {
  const parts: string[] = []
  if ((def.mod ?? 'ctrl') !== 'none') parts.push('⌘/Ctrl')
  if (def.shift) parts.push('Shift')
  if (def.alt) parts.push('Alt')
  parts.push(def.key.length === 1 ? def.key.toUpperCase() : def.key)
  return parts.join(' + ')
}

/** 在 setup 中注册（卸载自动注销）。 */
export function useShortcuts(defs: ShortcutDef[]): void {
  const unregisters = defs.map(registerShortcut)
  onBeforeUnmount(() => {
    for (const u of unregisters) u()
  })
}
