/**
 * EndpointEditor 回归测试：打开接口不得被误判为「有改动」。
 *
 * 背景：method watcher 监听 draft.method，但打开/切换接口时 draft 从
 * null → 有值，method 必然「变化」一次——旧实现把它当作用户改方法，
 * 回写 applyMethodDefaults 副作用（body 初始化 / Content-Type / active_tab），
 * 刚打开的接口立刻 isDirty。修复后：同一接口内的 method 变化才应用默认。
 */
import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import EndpointEditor from './EndpointEditor.vue'
import { useWorkspaceStore } from '../stores/workspace'
import { makeDraft } from '../testUtils/draftFixture'
import type { Endpoint } from '../types/foxApi'

vi.mock('../composables/useFoxApi', () => ({
  useFoxApi: () => new Proxy({}, { get: () => () => Promise.resolve(null) }),
}))

function endpoint(method: Endpoint['method']): Endpoint {
  return makeDraft({ id: 'ep-1', name: '新建宠物', method, path: '/pets' })
}

async function mountEditor() {
  setActivePinia(createPinia())
  const store = useWorkspaceStore()
  store.project = {
    id: 'proj-test-1',
    name: 'P',
    description: '',
    variables: {},
    created_at: '2026-01-01T00:00:00.000Z',
    updated_at: '2026-01-01T00:00:00.000Z',
  }
  const ep = endpoint('GET')
  store.endpoints = [ep]
  const wrapper = mount(EndpointEditor, { shallow: true })
  await nextTick()
  return { wrapper, store, ep }
}

describe('EndpointEditor：打开接口不误报脏', () => {
  it('打开 GET 接口后 isDirty 为 false（草稿不被回写）', async () => {
    const { wrapper, store, ep } = await mountEditor()

    store.openEndpoint(ep)
    await nextTick()
    await nextTick()

    expect(store.isDirty(ep.id)).toBe(false)
    const draft = store.draftOf(ep.id)
    expect(draft?.request.body).toEqual({ mode: 'none' })
    // 夹具无 active_tab 字段（undefined）：只要没被 watcher 写入即可
    expect(draft?.request.active_tab ?? null).toBeNull()
    wrapper.unmount()
  })

  it('同一接口内手动改 Method 仍应用智能默认（body 初始化 + active_tab）', async () => {
    const { wrapper, store, ep } = await mountEditor()

    store.openEndpoint(ep)
    await nextTick()

    // 模拟用户在方法选择器 GET → POST（id 不变 → 视为用户编辑）
    const draft = store.draftOf(ep.id)!
    expect(draft.method).toBe('GET')
    draft.method = 'POST'
    await vi.waitFor(() => expect(store.isDirty(ep.id)).toBe(true))

    expect(draft.request.body).toMatchObject({ mode: 'json' })
    expect(draft.request.active_tab).toBe('body')
    wrapper.unmount()
  })

  it('回归：GET→POST→GET 往返后不再显示「有改动」', async () => {
    const { wrapper, store, ep } = await mountEditor()

    store.openEndpoint(ep)
    await nextTick()
    const draft = store.draftOf(ep.id)!

    draft.method = 'POST'
    await vi.waitFor(() => expect(store.isDirty(ep.id)).toBe(true))

    draft.method = 'GET'
    await vi.waitFor(() => expect(store.isDirty(ep.id)).toBe(false))

    // request 完全还原为保存态（body / active_tab / POST 附加的 Content-Type 头被撤销）
    expect(draft.request.body).toEqual({ mode: 'none' })
    expect(draft.request.active_tab ?? null).toBeNull()
    expect(draft.request.headers.some((h) => h.key === 'Content-Type')).toBe(false)
    wrapper.unmount()
  })

  it('往返还原即使中途改过 body 内容也整体回到快照（用户语义：改回=没改）', async () => {
    const { wrapper, store, ep } = await mountEditor()

    store.openEndpoint(ep)
    await nextTick()
    const draft = store.draftOf(ep.id)!

    draft.method = 'POST'
    await vi.waitFor(() => expect(draft.request.body).toMatchObject({ mode: 'json' }))
    draft.request.body = { mode: 'json', raw: '{"typed":1}' }

    draft.method = 'GET'
    await vi.waitFor(() => expect(store.isDirty(ep.id)).toBe(false))
    expect(draft.request.body).toEqual({ mode: 'none' })
    wrapper.unmount()
  })

  it('保存后再改回原方法：与新的保存态不同，仍正确显示「有改动」', async () => {
    const { wrapper, store, ep } = await mountEditor()

    store.openEndpoint(ep)
    await nextTick()
    const draft = store.draftOf(ep.id)!

    draft.method = 'POST'
    await vi.waitFor(() => expect(store.isDirty(ep.id)).toBe(true))

    // 模拟保存：保存态同步为当前草稿（POST + json body）
    const savedNow = JSON.parse(JSON.stringify(draft)) as Endpoint
    store.endpoints = [savedNow]

    draft.method = 'GET'
    await vi.waitFor(() => expect(store.isDirty(ep.id)).toBe(true))
    expect(draft.request.body).toEqual({ mode: 'none' })
    wrapper.unmount()
  })
})
