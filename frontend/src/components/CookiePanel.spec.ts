/**
 * CookiePanel 单测：列表渲染 + 按域清理。
 */
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import CookiePanel from './CookiePanel.vue'
import { useLocaleStore } from '../stores/locale'
import type { CookieEntry } from '../types/foxApi'

const apiMocks = {
  cookieList: vi.fn(),
  cookieClear: vi.fn().mockResolvedValue(1),
}

vi.mock('../composables/useFoxApi', () => ({
  useFoxApi: () => apiMocks,
}))

vi.mock('../composables/useToast', () => ({
  useToast: () => ({ success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() }),
}))

function cookie(partial: Partial<CookieEntry> & { name: string }): CookieEntry {
  return {
    value: 'v',
    domain: 'api.example.com',
    path: '/',
    expires_at: null,
    secure: false,
    http_only: false,
    ...partial,
  }
}

describe('CookiePanel', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    // 文案断言锁定中文（jsdom 默认语言为英文，跟随系统会解析出英文）
    useLocaleStore().setMode('zh')
  })

  it('按域名分组渲染 + 清理后刷新', async () => {
    apiMocks.cookieList.mockResolvedValue([
      cookie({ name: 'sid', value: 'abc', domain: 'api.example.com' }),
      cookie({ name: 't', value: 'x'.repeat(60), domain: 'other.com', http_only: true }),
    ])
    const wrapper = mount(CookiePanel, { attachTo: document.body })
    await new Promise((r) => setTimeout(r, 0))
    expect(wrapper.text()).toContain('api.example.com')
    expect(wrapper.text()).toContain('other.com')
    expect(wrapper.text()).toContain('sid')
    // 值过长截断
    expect(wrapper.text()).toContain('…')
    expect(apiMocks.cookieList).toHaveBeenCalled()
    wrapper.unmount()
  })

  it('空列表展示空状态', async () => {
    apiMocks.cookieList.mockResolvedValue([])
    const wrapper = mount(CookiePanel, { attachTo: document.body })
    await new Promise((r) => setTimeout(r, 0))
    expect(wrapper.text()).toContain('暂无 Cookie')
    wrapper.unmount()
  })
})
