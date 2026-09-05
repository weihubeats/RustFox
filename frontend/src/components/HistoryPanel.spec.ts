/**
 * HistoryPanel 单测：关键字搜索 + 状态筛选。
 */
import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import HistoryPanel from './HistoryPanel.vue'
import { useLocaleStore } from '../stores/locale'
import { useWorkspaceStore } from '../stores/workspace'
import type { RequestHistory } from '../types/foxApi'

function history(partial: Partial<RequestHistory> & { id: string }): RequestHistory {
  return {
    project_id: 'p-1',
    endpoint_id: null,
    method: 'GET',
    url: 'https://api.example.com/users',
    status: 200,
    duration_ms: 12,
    request_summary_json: '{}',
    response_summary_json: '{}',
    created_at: '2026-01-01T00:00:00.000Z',
    ...partial,
  }
}

function mountPanel() {
  setActivePinia(createPinia())
  // 文案断言锁定中文（jsdom 默认语言为英文，跟随系统会解析出英文）
  useLocaleStore().setMode('zh')
  const store = useWorkspaceStore()
  store.histories = [
    history({ id: 'h-1', url: 'https://api.example.com/users', status: 200 }),
    history({ id: 'h-2', url: 'https://api.example.com/orders', status: 500, method: 'POST' }),
  ]
  return mount(HistoryPanel, { attachTo: document.body })
}

describe('HistoryPanel', () => {
  it('关键字过滤 URL/方法/状态码', async () => {
    const wrapper = mountPanel()
    expect(wrapper.findAll('.hp-row')).toHaveLength(2)
    await wrapper.find('.hp-search').setValue('orders')
    expect(wrapper.findAll('.hp-row')).toHaveLength(1)
    expect(wrapper.text()).toContain('/orders')
    await wrapper.find('.hp-search').setValue('500')
    expect(wrapper.findAll('.hp-row')).toHaveLength(1)
    await wrapper.find('.hp-search').setValue('no-such-thing')
    expect(wrapper.findAll('.hp-row')).toHaveLength(0)
    expect(wrapper.text()).toContain('无匹配记录')
    wrapper.unmount()
  })

  it('状态筛选在 全部/2xx/4xx5xx 间循环', async () => {
    const wrapper = mountPanel()
    const btn = wrapper.find('.hp-status-filter')
    expect(btn.text()).toBe('全部')
    await btn.trigger('click')
    expect(btn.text()).toBe('2xx')
    expect(wrapper.findAll('.hp-row')).toHaveLength(1)
    await btn.trigger('click')
    expect(btn.text()).toBe('4xx5xx')
    expect(wrapper.findAll('.hp-row')).toHaveLength(1)
    await btn.trigger('click')
    expect(btn.text()).toBe('全部')
    expect(wrapper.findAll('.hp-row')).toHaveLength(2)
    wrapper.unmount()
  })
})
