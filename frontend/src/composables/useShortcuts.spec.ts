/**
 * useShortcuts 单测：注册 / 匹配 / 注销 / 输入框隔离。
 */
import { describe, expect, it, vi } from 'vitest'
import { registerShortcut, shortcutGroups, shortcutLabel } from './useShortcuts'

function keydown(init: KeyboardEventInit): void {
  window.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }))
}

describe('useShortcuts', () => {
  it('修饰键 + 主键匹配并 preventDefault', () => {
    const handler = vi.fn()
    const unregister = registerShortcut({
      id: 'test.save',
      key: 's',
      group: '测试',
      description: '保存',
      handler,
    })
    keydown({ key: 's', ctrlKey: true })
    expect(handler).toHaveBeenCalledTimes(1)
    unregister()
    keydown({ key: 's', ctrlKey: true })
    expect(handler).toHaveBeenCalledTimes(1)
  })

  it('输入框内默认不触发，inInput 显式开启才触发', () => {
    const outer = vi.fn()
    const inner = vi.fn()
    const u1 = registerShortcut({ id: 't.outer', key: 'k', group: 'G', description: 'd', handler: outer })
    const u2 = registerShortcut({
      id: 't.inner',
      key: 'j',
      group: 'G',
      description: 'd',
      inInput: true,
      handler: inner,
    })
    const input = document.createElement('input')
    document.body.appendChild(input)
    // vitest 里直接派发带 target 的事件
    input.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: 'k', ctrlKey: true }))
    expect(outer).not.toHaveBeenCalled()
    input.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: 'j', ctrlKey: true }))
    expect(inner).toHaveBeenCalledTimes(1)
    document.body.removeChild(input)
    u1()
    u2()
  })

  it('帮助数据源按组聚合 + 按键文案', () => {
    const u = registerShortcut({ id: 't.help', key: 'Enter', group: '测试组', description: '回车', handler: () => {} })
    const groups = shortcutGroups()
    const g = groups.find((x) => x.group === '测试组')
    expect(g?.items.map((i) => i.id)).toContain('t.help')
    expect(shortcutLabel({ id: 'x', key: 's', group: 'g', description: 'd', handler: () => {} })).toBe(
      '⌘/Ctrl + S',
    )
    u()
  })
})
