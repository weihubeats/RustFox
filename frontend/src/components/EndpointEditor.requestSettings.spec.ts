/**
 * EndpointEditor 单测：配置区「Path」页签与底部请求设置行（单请求超时 / 跟随重定向）。
 * 两者都绑定 draft.request，走既有脏检查与保存链路，故只断言字段落值。
 */
import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import EndpointEditor from './EndpointEditor.vue'
import CustomNumberInput from './ui/CustomNumberInput.vue'
import { useWorkspaceStore } from '../stores/workspace'
import { useLocaleStore } from '../stores/locale'
import { makeDraft } from '../testUtils/draftFixture'
import type { Endpoint } from '../types/foxApi'

vi.mock('../composables/useFoxApi', () => ({
  useFoxApi: () => new Proxy({}, { get: () => () => Promise.resolve(null) }),
}))

async function mountEditor(prepare?: (ep: Endpoint) => void) {
  setActivePinia(createPinia())
  // 文案断言锁定中文（jsdom 默认语言为英文，跟随系统会解析出英文）
  useLocaleStore().setMode('zh')
  const store = useWorkspaceStore()
  store.project = {
    id: 'proj-test-1',
    name: 'P',
    description: '',
    variables: {},
    created_at: '2026-01-01T00:00:00.000Z',
    updated_at: '2026-01-01T00:00:00.000Z',
  }
  const ep = makeDraft({ id: 'ep-1', name: '宠物', method: 'GET', path: '/pets/:id' })
  prepare?.(ep)
  store.endpoints = [ep]
  // 全量挂载：Path 页签由子组件 Tabs 渲染，shallow 会把它 stub 掉；
  // 全局指令（main.ts 注册）在单测里空实现，避免 unresolved 警告
  const wrapper = mount(EndpointEditor, {
    global: { directives: { 'tooltip-overflow': {}, 'focus-end': {} } },
  })
  await nextTick()
  store.openEndpoint(ep)
  await nextTick()
  await nextTick()
  return { wrapper, store, ep }
}

describe('EndpointEditor：Path 页签', () => {
  it('页签列表含 Path，角标跟随 path_variables 条数；点击即切换', async () => {
    const { wrapper } = await mountEditor((ep) => {
      ep.request.path_variables = [
        { key: 'id', value: '1', enabled: true, description: '' },
        { key: 'scope', value: '', enabled: false, description: '' },
      ]
    })

    const pathTab = wrapper
      .findAll('.tabs .tab')
      .find((t) => t.find('.tab-label').text() === 'Path')
    expect(pathTab).toBeTruthy()
    expect(pathTab!.find('.tab-badge').text()).toBe('2')

    await pathTab!.trigger('click')
    await nextTick()
    const active = wrapper
      .findAll('.tabs .tab')
      .find((t) => t.attributes('aria-selected') === 'true')
    expect(active?.find('.tab-label').text()).toBe('Path')
    wrapper.unmount()
  })
})

describe('EndpointEditor：请求设置行', () => {
  it('超时输入写入 request.timeout_ms：空串→null，非法输入不写回', async () => {
    const { wrapper, store } = await mountEditor()
    const input = wrapper.findComponent(CustomNumberInput)
    expect(input.exists()).toBe(true)
    expect(input.props('modelValue')).toBe(30000)
    expect(wrapper.find('.req-settings').text()).toContain('毫秒')

    await input.vm.$emit('update:modelValue', '1500')
    expect(store.draftOf('ep-1')!.request.timeout_ms).toBe(1500)

    await input.vm.$emit('update:modelValue', '')
    expect(store.draftOf('ep-1')!.request.timeout_ms).toBeNull()

    await input.vm.$emit('update:modelValue', 2000)
    await input.vm.$emit('update:modelValue', 'abc')
    expect(store.draftOf('ep-1')!.request.timeout_ms).toBe(2000)
    wrapper.unmount()
  })

  it('「跟随重定向」复选直接驱动 request.follow_redirects', async () => {
    const { wrapper, store } = await mountEditor()
    const checkbox = wrapper.find('.rs-check input')
    expect(checkbox.exists()).toBe(true)
    expect(wrapper.find('.req-settings').text()).toContain('跟随重定向')
    expect(store.draftOf('ep-1')!.request.follow_redirects).toBe(true)

    // setChecked 在当前 VTU 类型下不可用，直接改值 + 派发 change（与原生行为一致）
    const box = checkbox.element as HTMLInputElement
    box.checked = false
    await checkbox.trigger('change')
    expect(store.draftOf('ep-1')!.request.follow_redirects).toBe(false)
    box.checked = true
    await checkbox.trigger('change')
    expect(store.draftOf('ep-1')!.request.follow_redirects).toBe(true)
    wrapper.unmount()
  })
})
