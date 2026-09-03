/**
 * RealtimeView 单测：WS/SSE 双页签渲染与切换（不建真实连接）。
 */
import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('vue-router', () => ({ useRouter: () => ({ push: vi.fn() }) }))

import RealtimeView from './RealtimeView.vue'

function mountView() {
  setActivePinia(createPinia())
  return mount(RealtimeView, { attachTo: document.body })
}

describe('RealtimeView', () => {
  it('默认展示 WebSocket 页（未连接态 + 空日志提示）', () => {
    const wrapper = mountView()
    expect(wrapper.text()).toContain('未连接')
    expect(wrapper.text()).toContain('连接后在此查看收发的帧')
    expect(
      (wrapper.find('input[placeholder="ws://127.0.0.1:4010/socket"]').element as HTMLInputElement)
        .value,
    ).toBe('ws://127.0.0.1:4010')
    wrapper.unmount()
  })

  it('切换到 SSE 页签展示订阅入口与空日志提示', async () => {
    const wrapper = mountView()
    const tabs = wrapper.findAll('[role="tab"]')
    expect(tabs.map((t) => t.text())).toEqual(['WebSocket', 'SSE'])
    await tabs[1].trigger('click')
    expect(wrapper.text()).toContain('未订阅')
    expect(wrapper.text()).toContain('订阅后在此查看事件流')
    wrapper.unmount()
  })
})
