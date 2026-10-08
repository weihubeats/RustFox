/**
 * EndpointEditor 地址栏输入单测：
 *
 * 回归 bug——旧 urlPath setter 逐键归一化，输入 `h` 瞬间被改写成 `/h`，
 * 想输自定义域名 `http://…` 打到一半已被加斜杠，绝对地址分支永远无法命中。
 * 修复后：逐键原文存储，归一化只在提交时机（失焦 / 发送 / 保存 / 粘贴）执行。
 */
import { mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import EndpointEditor from './EndpointEditor.vue'
import { useLocaleStore } from '../stores/locale'
import { useWorkspaceStore } from '../stores/workspace'
import { makeDraft } from '../testUtils/draftFixture'
import { collectErrors, stubScrollIntoView } from '../testUtils/componentTest'
import type { Environment, Endpoint } from '../types/foxApi'

/** foxApi 全量 mock（口径同 EndpointEditor.response.spec：挂载期懒加载接口全数兜底）。 */
const apiMock = vi.hoisted(() => ({
  executeRequest: vi.fn(async () => {
    throw new Error('not used')
  }),
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

const endpoint: Endpoint = makeDraft({ id: 'ep-url', name: '地址栏', path: '/' })

async function mountEditor() {
  const store = useWorkspaceStore()
  // 文案断言锁定中文（jsdom 默认语言为英文，跟随系统会解析出英文）
  useLocaleStore().setMode('zh')
  store.project = {
    id: 'proj-url-1',
    name: 'P',
    description: '',
    variables: {},
    created_at: '2026-01-01T00:00:00.000Z',
    updated_at: '2026-01-01T00:00:00.000Z',
  }
  store.endpoints = [endpoint]
  const wrapper = mount(EndpointEditor, { shallow: true })
  await nextTick()
  store.openEndpoint(endpoint)
  await nextTick()
  return { wrapper, store }
}

/** 注入「有 base_url 的激活环境」（口径同 workspace.spec）。 */
function activateEnvWithBase(store: ReturnType<typeof useWorkspaceStore>): Environment {
  const now = new Date().toISOString()
  const env: Environment = {
    id: 'env-1',
    project_id: 'proj-url-1',
    name: '测试',
    base_url: 'https://env.example.com',
    variables: [],
    created_at: now,
    updated_at: now,
  }
  store.environments.push(env)
  store.activeEnvId = 'env-1'
  return env
}

describe('EndpointEditor：地址栏逐键不改写，提交才归一化', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    stubScrollIntoView()
  })

  it('输入 h 不再被改写成 /h；失焦提交才补斜杠（回归：逐键归一化）', async () => {
    const errors = collectErrors()
    const { wrapper, store } = await mountEditor()
    const input = wrapper.find('.url-input')
    expect((input.element as HTMLInputElement).value).toBe('/')

    // 清空（旧实现 setter 忽略空串，输入框清不掉；新实现允许）。
    await input.setValue('')
    expect(store.activeEndpoint!.path).toBe('')

    // 逐键输入：显示与模型都保持原文，不出现 `/h`。
    await input.setValue('h')
    expect((input.element as HTMLInputElement).value).toBe('h')
    expect(store.activeEndpoint!.path).toBe('h')

    // 失焦提交 → 归一为 /h。
    await input.trigger('blur')
    await nextTick()
    expect(store.activeEndpoint!.path).toBe('/h')
    expect((input.element as HTMLInputElement).value).toBe('/h')

    wrapper.unmount()
    errors.restore()
    expect(errors.errors).toEqual([])
  })

  it('环境激活时逐键输完整自定义域名：中间态不拆不加斜杠，失焦整条存 path 且环境不被写', async () => {
    const errors = collectErrors()
    const { wrapper, store } = await mountEditor()
    const env = activateEnvWithBase(store)
    const input = wrapper.find('.url-input')

    // 逐键中间态：路径原样，不被加 `/`、不被提前拆 origin。
    for (const partial of [
      'h',
      'ht',
      'http://a',
      'http://api.custom.com',
      'http://api.custom.com/v1/users',
    ]) {
      await input.setValue(partial)
      expect(store.activeEndpoint!.path).toBe(partial)
    }

    // 失焦提交：环境前缀分支 → origin+path 整条存入 path（发送直达自定义域名）。
    await input.trigger('blur')
    await nextTick()
    expect(store.activeEndpoint!.path).toBe('http://api.custom.com/v1/users')
    // 共享环境的 base_url 不被导入污染。
    expect(env.base_url).toBe('https://env.example.com')
    expect(store.activeEnvId).toBe('env-1')

    wrapper.unmount()
    errors.restore()
    expect(errors.errors).toEqual([])
  })

  it('无环境输完整域名：失焦拆分——origin 进前缀、path 留相对、query 并入参数', async () => {
    const errors = collectErrors()
    const { wrapper, store } = await mountEditor()
    const input = wrapper.find('.url-input')

    await input.setValue('https://api.custom.com/v1/users?page=2')
    // 提交前：原文存储。
    expect(store.activeEndpoint!.path).toBe('https://api.custom.com/v1/users?page=2')

    await input.trigger('blur')
    await nextTick()
    expect(store.activeEndpoint!.path).toBe('/v1/users')
    expect(store.sessionBaseUrl).toBe('https://api.custom.com')
    expect(store.activeEndpoint!.request.params).toEqual(
      expect.arrayContaining([expect.objectContaining({ key: 'page', value: '2' })]),
    )

    wrapper.unmount()
    errors.restore()
    expect(errors.errors).toEqual([])
  })

  it('粘贴完整 URL：无需失焦立即拆分，query 并入参数（保持旧粘贴体验）', async () => {
    const errors = collectErrors()
    const { wrapper, store } = await mountEditor()
    const input = wrapper.find('.url-input')

    const ev = new Event('paste', { bubbles: true, cancelable: true })
    Object.defineProperty(ev, 'clipboardData', {
      value: { getData: () => 'https://p.example.com/a?k=1' },
    })
    input.element.dispatchEvent(ev)
    await nextTick()

    expect(store.activeEndpoint!.path).toBe('/a')
    expect(store.sessionBaseUrl).toBe('https://p.example.com')
    expect(store.activeEndpoint!.request.params).toEqual(
      expect.arrayContaining([expect.objectContaining({ key: 'k', value: '1' })]),
    )

    wrapper.unmount()
    errors.restore()
    expect(errors.errors).toEqual([])
  })

  it('gRPC 地址逐键输入：失焦不加 / 前缀（host:port 原样保留）', async () => {
    const errors = collectErrors()
    const store = useWorkspaceStore()
    useLocaleStore().setMode('zh')
    store.project = {
      id: 'proj-url-1',
      name: 'P',
      description: '',
      variables: {},
      created_at: '2026-01-01T00:00:00.000Z',
      updated_at: '2026-01-01T00:00:00.000Z',
    }
    const grpcEp = makeDraft({ id: 'ep-grpc-url', name: 'gRPC', method: 'GRPC', path: '' })
    store.endpoints = [grpcEp]
    const wrapper = mount(EndpointEditor, { shallow: true })
    await nextTick()
    store.openEndpoint(grpcEp)
    await nextTick()

    const input = wrapper.find('.url-input')
    await input.setValue('localhost:5051')
    expect(store.activeEndpoint!.path).toBe('localhost:5051')
    await input.trigger('blur')
    await nextTick()
    expect(store.activeEndpoint!.path).toBe('localhost:5051')

    wrapper.unmount()
    errors.restore()
    expect(errors.errors).toEqual([])
  })
})
