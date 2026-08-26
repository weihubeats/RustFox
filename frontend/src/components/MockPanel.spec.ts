/**
 * MockPanel 单测：规则列表渲染 + 「打开 Mock 管理」按钮 emit openManager。
 * store 以模块级 mock 替换（无需 Pinia 实例）。
 */
import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import MockPanel from './MockPanel.vue'
import { makeDraft } from '../testUtils/draftFixture'

vi.mock('../stores/workspace', () => ({
  useWorkspaceStore: () => ({
    project: { id: 'p-1', name: 'P' },
  }),
}))

vi.mock('../composables/useFoxApi', () => ({
  useFoxApi: () => ({
    listMockRules: vi.fn().mockResolvedValue([]),
  }),
}))

describe('MockPanel', () => {
  it('点击「打开 Mock 管理」应触发 openManager 事件', async () => {
    const wrapper = mount(MockPanel, { props: { draft: makeDraft({ id: 'ep-1' }) } })
    const btn = wrapper.find('button')
    expect(btn.text()).toContain('打开 Mock 管理')
    await btn.trigger('click')
    expect(wrapper.emitted('openManager')).toHaveLength(1)
  })
})
