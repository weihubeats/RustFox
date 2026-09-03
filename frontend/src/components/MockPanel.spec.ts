/**
 * MockPanel 单测：规则列表渲染 + 「打开 Mock 管理」按钮 emit openManager + 热重载。
 * store / api 以模块级 mock 替换（无需 Pinia 实例）。
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

const apiMocks = {
  listMockRules: vi.fn().mockResolvedValue([]),
  mockReload: vi.fn().mockResolvedValue(7),
}

vi.mock('../composables/useFoxApi', () => ({
  useFoxApi: () => apiMocks,
}))

describe('MockPanel', () => {
  it('点击「打开 Mock 管理」应触发 openManager 事件', async () => {
    const wrapper = mount(MockPanel, { props: { draft: makeDraft({ id: 'ep-1' }) } })
    const btn = wrapper.findAll('button').find((b) => b.text().includes('打开 Mock 管理'))
    expect(btn?.text()).toContain('打开 Mock 管理')
    await btn!.trigger('click')
    expect(wrapper.emitted('openManager')).toHaveLength(1)
  })

  it('热重载按钮调用 mockReload', async () => {
    apiMocks.mockReload.mockClear()
    const wrapper = mount(MockPanel, { props: { draft: makeDraft({ id: 'ep-1' }) } })
    const btn = wrapper.findAll('button').find((b) => b.text().includes('热重载'))
    expect(btn).toBeTruthy()
    await btn!.trigger('click')
    expect(apiMocks.mockReload).toHaveBeenCalledTimes(1)
  })
})
