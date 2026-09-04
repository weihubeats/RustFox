/**
 * EndpointEditor 回归测试：每个接口只显示自己的请求结果。
 *
 * 背景：EndpointEditor 单实例常驻，响应曾用单个 ref 存储——切到另一个
 * 接口会看到上一个接口的响应；请求在途时切换还会让旧接口的返回错落到
 * 当前接口（「A 请求却显示 B 结果」）。修复后按接口 id 分桶（Map）。
 * 本测试锁定：切换接口不串响应、每个接口只显示自己的最后一次结果。
 */
import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import EndpointEditor from './EndpointEditor.vue'
import ResponsePanel from './ResponsePanel.vue'
import { useWorkspaceStore } from '../stores/workspace'
import { makeDraft } from '../testUtils/draftFixture'
import { collectErrors, stubScrollIntoView } from '../testUtils/componentTest'
import type { Endpoint, ExecuteResponse } from '../types/foxApi'

/** foxApi 全量 mock：executeRequest 按 endpoint_id 返回对应响应体，便于断言「谁的响应」。 */
const apiMock = vi.hoisted(() => ({
  executeRequest: vi.fn(),
  listExamples: vi.fn(async () => []),
  listRequestExamples: vi.fn(async () => []),
  listTestCases: vi.fn(async () => []),
  listHistories: vi.fn(async () => []),
  listEnvironments: vi.fn(async () => []),
  getActiveEnvironment: vi.fn(async () => null),
  getGlobalVariables: vi.fn(async () => []),
  getGlobalParams: vi.fn(async () => []),
  saveRequestExample: vi.fn(),
  saveExample: vi.fn(),
}))

vi.mock('../composables/useFoxApi', () => ({ useFoxApi: () => apiMock }))

function makeResponse(tag: string): ExecuteResponse {
  return {
    status: 200,
    headers: [['content-type', 'application/json']],
    body: JSON.stringify({ from: tag }),
    content_type: 'application/json',
    duration_ms: 10,
    size_bytes: 16,
    truncated: false,
  }
}

const endpointA: Endpoint = makeDraft({ id: 'ep-a', name: '接口 A', path: '/a' })
const endpointB: Endpoint = makeDraft({ id: 'ep-b', name: '接口 B', path: '/b' })

async function mountEditor() {
  const store = useWorkspaceStore()
  store.project = {
    id: 'proj-test-1',
    name: 'P',
    description: '',
    variables: {},
    created_at: '2026-01-01T00:00:00.000Z',
    updated_at: '2026-01-01T00:00:00.000Z',
  }
  store.endpoints = [endpointA, endpointB]
  const wrapper = mount(EndpointEditor, { shallow: true })
  await nextTick()
  return { wrapper, store }
}

async function sendCurrent(wrapper: ReturnType<typeof mount>): Promise<void> {
  await wrapper.find('.bar-send').trigger('click')
  await flushPromises()
  await vi.waitFor(() => {
    expect(wrapper.findComponent(ResponsePanel).exists()).toBe(true)
  })
}

describe('EndpointEditor：接口响应按 id 隔离', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    stubScrollIntoView()
    apiMock.executeRequest.mockImplementation((args: { endpoint_id: string | null }) =>
      Promise.resolve(makeResponse(args.endpoint_id ?? 'unknown')),
    )
  })

  it('A 请求后切到未请求的 B：不显示 A 的响应，而是空态', async () => {
    const errors = collectErrors()
    const { wrapper, store } = await mountEditor()

    store.openEndpoint(endpointA)
    await nextTick()
    await sendCurrent(wrapper)
    expect(wrapper.findComponent(ResponsePanel).props('response').body).toContain('ep-a')

    // 切到 B（从未请求）：不得残留 A 的响应。
    store.openEndpoint(endpointB)
    await nextTick()
    expect(wrapper.findComponent(ResponsePanel).exists()).toBe(false)
    expect(wrapper.find('.response-hint').exists()).toBe(true)

    wrapper.unmount()
    errors.restore()
    expect(errors.errors).toEqual([])
  })

  it('A→B 各自发送：各显示自己的响应，切回 A 仍显示 A 的结果', async () => {
    const errors = collectErrors()
    const { wrapper, store } = await mountEditor()

    store.openEndpoint(endpointA)
    await nextTick()
    await sendCurrent(wrapper)
    expect(wrapper.findComponent(ResponsePanel).props('response').body).toContain('ep-a')

    store.openEndpoint(endpointB)
    await nextTick()
    await sendCurrent(wrapper)
    expect(wrapper.findComponent(ResponsePanel).props('response').body).toContain('ep-b')

    // 切回 A：显示 A 自己的结果，不是 B 的。
    store.openEndpoint(endpointA)
    await nextTick()
    expect(wrapper.findComponent(ResponsePanel).props('response').body).toContain('ep-a')

    wrapper.unmount()
    errors.restore()
    expect(errors.errors).toEqual([])
  })

  it('A 请求在途时切到 B：A 响应晚到也不串到 B（B 保持空态）', async () => {
    const errors = collectErrors()
    let resolveA!: (v: ExecuteResponse) => void
    apiMock.executeRequest.mockImplementation((args: { endpoint_id: string | null }) =>
      args.endpoint_id === 'ep-a'
        ? new Promise<ExecuteResponse>((r) => {
            resolveA = r
          })
        : Promise.resolve(makeResponse('ep-b')),
    )
    const { wrapper, store } = await mountEditor()

    store.openEndpoint(endpointA)
    await nextTick()
    // 发起 A 的请求但不等待返回（promise 挂起）。
    const sendP = wrapper.find('.bar-send').trigger('click')

    // 在途切换到 B：B 从未请求，应显示空态。
    store.openEndpoint(endpointB)
    await nextTick()
    expect(wrapper.findComponent(ResponsePanel).exists()).toBe(false)
    expect(wrapper.find('.response-hint').exists()).toBe(true)

    // A 的响应此刻返回——必须落到 A 桶，B 不应被串改。
    resolveA(makeResponse('ep-a-late'))
    await sendP
    await flushPromises()
    await nextTick()

    // B 仍保持空态，没有 A 的响应串进来。
    expect(wrapper.findComponent(ResponsePanel).exists()).toBe(false)
    expect(wrapper.find('.response-hint').exists()).toBe(true)

    // 切回 A：看到 A 迟到的响应。
    store.openEndpoint(endpointA)
    await nextTick()
    expect(wrapper.findComponent(ResponsePanel).props('response').body).toContain('ep-a-late')

    wrapper.unmount()
    errors.restore()
    expect(errors.errors).toEqual([])
  })

  it('A 请求失败只影响 A：B 显示自己的成功结果', async () => {
    const errors = collectErrors()
    apiMock.executeRequest.mockImplementation((args: { endpoint_id: string | null }) =>
      args.endpoint_id === 'ep-a'
        ? Promise.reject(Object.assign(new Error('boom'), { code: 'HTTP' }))
        : Promise.resolve(makeResponse('ep-b')),
    )
    const { wrapper, store } = await mountEditor()

    store.openEndpoint(endpointA)
    await nextTick()
    await wrapper.find('.bar-send').trigger('click')
    await flushPromises()
    // A 显示错误态（response 清空），不残留上一次成功响应。
    expect(wrapper.findComponent(ResponsePanel).exists()).toBe(false)
    expect(wrapper.find('.send-error').exists()).toBe(true)
    expect(wrapper.find('.send-error').text()).toContain('boom')

    store.openEndpoint(endpointB)
    await nextTick()
    await sendCurrent(wrapper)
    expect(wrapper.findComponent(ResponsePanel).props('response').body).toContain('ep-b')

    wrapper.unmount()
    errors.restore()
    expect(errors.errors).toEqual([])
  })
})
