/**
 * workspace store 多项目快照切换单测：
 * 项目 A 的草稿/标签在切到 B 再切回后必须原样恢复；B 首次进入为全新态。
 * useFoxApi 以模块级 mock 替换（内存假后端）。
 */
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import type { CurlParsed, Endpoint, Environment, Project } from '../types/foxApi'

function makeProject(id: string, name: string): Project {
  const now = new Date().toISOString()
  return { id, name, description: '', variables: {}, created_at: now, updated_at: now }
}

function makeEndpoint(id: string, projectId: string, name: string, now = new Date().toISOString()): Endpoint {
  return {
    id,
    project_id: projectId,
    folder_id: null,
    name,
    method: 'GET',
    path: `/${name}`,
    description: '',
    status: 'designing',
    sort_order: 0,
    request: {
      params: [],
      headers: [],
      path_variables: [],
      auth: { type: 'none' },
      body: { mode: 'none' },
      active_tab: null,
      timeout_ms: 30000,
      follow_redirects: true,
      tests: null,
    },
    created_at: now,
    updated_at: now,
  }
}

const backend = vi.hoisted(() => {
  const projects: Project[] = []
  const endpointsByProject = new Map<string, Endpoint[]>()
  const envsByProject = new Map<string, Environment[]>()
  let activeId: string | null = null
  /** 历史接口行为控制（分页 / 失败提示用例）。 */
  const historyRows: { id: string }[] = []
  const historyCalls: (number | undefined)[] = []
  let historyImpl:
    | ((projectId: string, limit?: number, endpointId?: string | null) => Promise<{ id: string }[]>)
    | null = null
  return {
    projects,
    endpointsByProject,
    envsByProject,
    setActive: (id: string | null) => {
      activeId = id
    },
    active: () => activeId,
    historyRows,
    historyCalls,
    setHistoryImpl: (
      fn: ((projectId: string, limit?: number, endpointId?: string | null) => Promise<{ id: string }[]>) | null,
    ) => {
      historyImpl = fn
    },
    historyImpl: () => historyImpl,
  }
})

/** useToast 单例句柄（断言失败提示只出现一次时用）。 */
const toastMock = vi.hoisted(() => ({
  success: vi.fn(),
  error: vi.fn(),
  info: vi.fn(),
  warning: vi.fn(),
}))

vi.mock('../composables/useFoxApi', () => ({
  useFoxApi: () => ({
    getProjects: vi.fn().mockResolvedValue(backend.projects),
    getActiveProject: vi.fn().mockImplementation(async () => backend.projects.find((p) => p.id === backend.active()) ?? null),
    setActiveProject: vi.fn().mockImplementation(async (id: string | null) => {
      backend.setActive(id)
      return backend.projects.find((p) => p.id === id) ?? null
    }),
    listFolders: vi.fn().mockResolvedValue([]),
    listEndpoints: vi.fn().mockImplementation(async (pid: string) => backend.endpointsByProject.get(pid) ?? []),
    listEnvironments: vi.fn().mockImplementation(async () => []),
    getActiveEnvironment: vi.fn().mockResolvedValue(null),
    setActiveEnvironment: vi.fn().mockResolvedValue(undefined),
    getGlobalVariables: vi.fn().mockResolvedValue([]),
    saveGlobalVariables: vi.fn().mockResolvedValue(undefined),
    getGlobalParams: vi.fn().mockResolvedValue([]),
    saveGlobalParams: vi.fn().mockResolvedValue(undefined),
    listExamples: vi.fn().mockResolvedValue([]),
    listRequestExamples: vi.fn().mockResolvedValue([]),
    listTestCases: vi.fn().mockResolvedValue([]),
    listRequestHistories: vi
      .fn()
      .mockImplementation((projectId: string, limit?: number, endpointId?: string | null) => {
        backend.historyCalls.push(limit)
        const impl = backend.historyImpl()
        if (impl) return impl(projectId, limit, endpointId)
        return Promise.resolve(backend.historyRows.slice(0, limit ?? 50))
      }),
    saveEndpoint: vi.fn().mockImplementation(async (ep: Endpoint) => {
      const list = backend.endpointsByProject.get(ep.project_id) ?? []
      const idx = list.findIndex((x) => x.id === ep.id)
      if (idx === -1) list.push(ep)
      else list[idx] = ep
      return ep
    }),
  }),
}))

vi.mock('../composables/useToast', () => ({
  useToast: () => toastMock,
}))

import { useWorkspaceStore } from '../stores/workspace'
import { useLocaleStore } from '../stores/locale'

/** 测试环境的 localStorage 为残缺对象（真机 WebView 才有完整实现），stub 一个内存版。 */
beforeAll(() => {
  const mem = new Map<string, string>()
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => mem.get(k) ?? null,
    setItem: (k: string, v: string) => void mem.set(k, v),
    removeItem: (k: string) => void mem.delete(k),
  })
})

describe('workspace store 多项目快照切换', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    // 文案断言锁定中文（jsdom 默认语言为英文，跟随系统会解析出英文）
    useLocaleStore().setMode('zh')
    backend.projects.length = 0
    backend.endpointsByProject.clear()
    backend.envsByProject.clear()
    backend.setActive(null)
    const a = makeProject('p-a', '项目A')
    const b = makeProject('p-b', '项目B')
    backend.projects.push(a, b)
    backend.endpointsByProject.set('p-a', [makeEndpoint('ep-1', 'p-a', 'users')])
    backend.endpointsByProject.set('p-b', [makeEndpoint('ep-2', 'p-b', 'orders')])
    backend.envsByProject.set('p-a', [])
    backend.envsByProject.set('p-b', [])
  })

  it('切走再切回：草稿与标签原样恢复；新项目为全新态', async () => {
    const store = useWorkspaceStore()

    // 进入 A，打开接口并修改草稿（脏）
    backend.setActive('p-a')
    await store.init()
    expect(store.project?.id).toBe('p-a')
    store.openEndpoint(store.endpoints[0]!)
    store.draftOf('ep-1')!.name = '用户列表-改'

    // 切到 B：全新态（无标签），A 的草稿不丢
    await store.switchProject('p-b')
    expect(store.project?.id).toBe('p-b')
    expect(store.openTabs).toHaveLength(0)
    expect(store.openProjects.map((t) => t.id)).toEqual(['p-a', 'p-b'])

    // 切回 A：标签与脏草稿原样恢复
    await store.switchProject('p-a')
    expect(store.project?.id).toBe('p-a')
    expect(store.openTabs).toEqual(['ep-1'])
    expect(store.draftOf('ep-1')?.name).toBe('用户列表-改')
    expect(store.isDirty('ep-1')).toBe(true)

    // 关闭 A 标签：自动切到 B
    store.closeProjectTab('p-a')
    await vi.waitFor(() => expect(store.project?.id).toBe('p-b'))
    expect(store.openProjects.map((t) => t.id)).toEqual(['p-b'])

    // 关闭最后一个标签：project 清空（视图负责跳转）
    store.closeProjectTab('p-b')
    expect(store.project).toBeNull()
  })

  it('切换到不存在的项目：移除标签并抛错，当前项目不受影响', async () => {
    const store = useWorkspaceStore()
    backend.setActive('p-a')
    await store.init()
    await expect(store.switchProject('ghost')).rejects.toThrow('项目不存在')
    expect(store.project?.id).toBe('p-a')
    expect(store.openProjects.map((t) => t.id)).toEqual(['p-a'])
  })

  it('重启后从项目列表直进（不经过 init）：持久化的标签仍恢复', async () => {
    // 模拟上次会话打开过 A、B 两个标签
    localStorage.setItem('rustfox.open-projects', JSON.stringify(['p-a', 'p-b']))
    const store = useWorkspaceStore()

    // 用户在项目列表页点 B 进入（store 全新，未调 init）
    await store.switchProject('p-b')
    expect(store.project?.id).toBe('p-b')
    expect(store.openProjects.map((t) => t.id)).toEqual(['p-a', 'p-b'])
  })

  it('新建空接口默认干净：无改动关闭不确认；改动后才变脏', async () => {
    const store = useWorkspaceStore()
    backend.setActive('p-a')
    await store.init()
    store.openNewEndpoint(null)
    const id = store.activeTabId!
    // 刚 + 出来、一个字没改：干净（无小圆点、无关闭确认）
    expect(store.isDirty(id)).toBe(false)
    // 改名 → 脏（定版推进是 watcher 套 nextTick 的两跳，这里等两拍）
    const blankName = store.draftOf(id)!.name
    store.draftOf(id)!.name = '新建用户'
    await nextTick()
    await nextTick()
    expect(store.isDirty(id)).toBe(true)
    // 改回原样 → 干净
    store.draftOf(id)!.name = blankName
    await nextTick()
    await nextTick()
    expect(store.isDirty(id)).toBe(false)
    // 改路径 → 脏；保存后回归干净（创建快照使命完成）
    store.draftOf(id)!.path = '/users'
    await nextTick()
    await nextTick()
    expect(store.isDirty(id)).toBe(true)
    store.draftOf(id)!.name = '新建用户'
    expect(await store.saveActiveDraft()).toBe(true)
    expect(store.isDirty(id)).toBe(false)
  })

  it('批量关闭：closeOthers 保留当前，closeAll 清空并回退激活', async () => {
    const store = useWorkspaceStore()
    backend.setActive('p-a')
    await store.init()
    store.openEndpoint(store.endpoints[0]!)
    store.openNewEndpoint(null)
    store.openNewEndpoint(null)
    expect(store.openTabs).toHaveLength(3)
    const keep = store.activeTabId!
    expect(store.closeOtherTabs(keep)).toBe(2)
    expect(store.openTabs).toEqual([keep])
    expect(store.activeTabId).toBe(keep)
    expect(store.closeAllTabs()).toBe(1)
    expect(store.openTabs).toHaveLength(0)
    expect(store.activeTabId).toBeNull()
  })
})

describe('moveEndpoint：移动后打开草稿的 folder_id / sort_order 同步', () => {
  beforeEach(() => {
    // 文案经 i18n 取词，锁定中文（jsdom 默认语言为英文）
    useLocaleStore().setMode('zh')
    backend.setActive('p-a')
    // 固定时钟：isDirty 依赖草稿/已存的全字段 eq 比较，时间戳漂移会误判「脏」
    const T = '2026-01-01T00:00:00.000Z'
    backend.endpointsByProject.set('p-a', [
      { ...makeEndpoint('ep-a', 'p-a', 'in-a', T), folder_id: 'f-a', sort_order: 0 },
      { ...makeEndpoint('ep-other', 'p-a', 'also-in-a', T), folder_id: 'f-a', sort_order: 1 },
      { ...makeEndpoint('ep-b', 'p-a', 'in-b', T), folder_id: 'f-b', sort_order: 0 },
    ])
  })

  it('跨文件夹移动后保存：folder_id 保持新归属，不退回旧文件夹（回归 bug#保存回退）', async () => {
    const store = useWorkspaceStore()
    await store.init()
    store.openEndpoint(store.endpoints.find((e) => e.id === 'ep-a')!)

    await store.moveEndpoint('ep-a', 'f-b', 0)

    // 打开草稿的 folder_id 随移动同步；保存写库不再用旧归属覆盖
    expect(store.draftOf('ep-a')?.folder_id).toBe('f-b')
    expect(await store.saveActiveDraft()).toBe(true)
    const saved = backend.endpointsByProject.get('p-a')!.find((e) => e.id === 'ep-a')!
    expect(saved.folder_id).toBe('f-b')
  })

  it('同组内移动：打开草稿的 sort_order 同步，不产生假「脏」', async () => {
    const store = useWorkspaceStore()
    await store.init()
    store.openEndpoint(store.endpoints.find((e) => e.id === 'ep-a')!)
    store.openEndpoint(store.endpoints.find((e) => e.id === 'ep-other')!)

    // 把 ep-a 移到组尾：ep-other 前移
    await store.moveEndpoint('ep-a', 'f-a', 1)
    // dirtyTick 由 nextTick 推进，等其落定后 isDirty 才是稳定值
    await nextTick()

    expect(store.draftOf('ep-a')?.sort_order).toBe(1)
    expect(store.draftOf('ep-other')?.sort_order).toBe(0)
    expect(store.isDirty('ep-a')).toBe(false)
    expect(store.isDirty('ep-other')).toBe(false)
  })

  it('编辑 body 内容并保存：草稿与保存态深克隆解耦，保存后内容稳定不丢失', async () => {
    const store = useWorkspaceStore()
    await store.init()
    const ep = store.endpoints.find((e) => e.id === 'ep-a')!
    store.openEndpoint(ep)

    const draft = store.draftOf('ep-a')!
    draft.request.body = { mode: 'json', raw: '{"name":"fox","added":"s"}' }

    expect(store.isDirty('ep-a')).toBe(true)
    expect(await store.saveActiveDraft()).toBe(true)

    // 保存后草稿保持最新内容
    expect(store.draftOf('ep-a')?.request.body).toEqual({
      mode: 'json',
      raw: '{"name":"fox","added":"s"}',
    })
    expect(store.isDirty('ep-a')).toBe(false)
  })
})

describe('cURL 导入：环境前缀优先时不覆写环境，path 存完整 URL', () => {
  const parsed: CurlParsed = {
    url: 'https://httpbin.org/post?x=1',
    method: 'POST',
    headers: [{ key: 'Content-Type', value: 'application/json', enabled: true, description: '' }],
    body: null,
    auth: { type: 'none' },
  }

  beforeEach(async () => {
    setActivePinia(createPinia())
    useLocaleStore().setMode('zh')
    backend.projects.length = 0
    backend.endpointsByProject.clear()
    backend.envsByProject.clear()
    backend.projects.push(makeProject('p-a', '项目A'))
    backend.endpointsByProject.set('p-a', [])
    backend.envsByProject.set('p-a', [])
    backend.setActive('p-a')
  })

  /** 注入「有 base_url 的激活环境」：urlDomain 应为 {{base_url}}。 */
  function activateEnvWithBase(store: ReturnType<typeof useWorkspaceStore>): Environment {
    const now = new Date().toISOString()
    const env: Environment = {
      id: 'env-1',
      name: '测试',
      modules: [{ id: 'm-1', module_name: '默认', base_url: 'https://env.example.com', is_default: true }],
      variables: [],
      created_at: now,
      updated_at: now,
    }
    store.environments.push(env)
    store.activeEnvId = 'env-1'
    return env
  }

  it('激活环境导入 curl（弹窗路径 openCurlDraft）：path 为完整 URL，环境 base_url 原样', async () => {
    const store = useWorkspaceStore()
    await store.init()
    const env = activateEnvWithBase(store)
    expect(store.urlDomain).toBe('{{base_url}}')

    store.openCurlDraft(parsed, null)
    const id = store.activeTabId!

    // 地址栏应直达 httpbin（完整 URL 存 path），而非显示环境前缀 + /post
    expect(store.draftOf(id)!.path).toBe('https://httpbin.org/post')
    expect(store.draftOf(id)!.request.params).toEqual([
      { key: 'x', value: '1', enabled: true, description: '' },
    ])
    // 共享环境不被导入污染；展示前缀仍是环境变量
    expect(env.modules[0]!.base_url).toBe('https://env.example.com')
    expect(store.urlDomain).toBe('{{base_url}}')
    expect(store.sessionBaseUrl).toBe('https://httpbin.org')
  })

  it('无环境 base 导入 curl：path 相对，会话 Base URL 预填 origin', async () => {
    const store = useWorkspaceStore()
    await store.init()

    store.openCurlDraft(parsed, null)
    const id = store.activeTabId!

    expect(store.draftOf(id)!.path).toBe('/post')
    expect(store.sessionBaseUrl).toBe('https://httpbin.org')
    expect(store.urlDomain).toBe('https://httpbin.org')
  })

  it('地址栏粘贴回填已有草稿（环境激活）：同规则，path 为完整 URL、环境不被写', async () => {
    const store = useWorkspaceStore()
    await store.init()
    const env = activateEnvWithBase(store)
    store.openNewEndpoint(null)
    const id = store.activeTabId!

    store.applyCurlToDraft(id, parsed)

    expect(store.draftOf(id)!.path).toBe('https://httpbin.org/post')
    expect(store.draftOf(id)!.method).toBe('POST')
    expect(env.modules[0]!.base_url).toBe('https://env.example.com')
  })
})

describe('请求历史：增量分页与失败提示', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    useLocaleStore().setMode('zh')
    backend.projects.length = 0
    backend.endpointsByProject.clear()
    backend.envsByProject.clear()
    backend.setActive(null)
    backend.projects.push(makeProject('p-a', '项目A'), makeProject('p-b', '项目B'))
    backend.endpointsByProject.set('p-a', [])
    backend.endpointsByProject.set('p-b', [])
    backend.envsByProject.set('p-a', [])
    backend.envsByProject.set('p-b', [])
    backend.historyRows.length = 0
    backend.historyCalls.length = 0
    backend.setHistoryImpl(null)
    toastMock.error.mockClear()
    toastMock.success.mockClear()
  })

  function seedHistory(n: number): void {
    backend.historyRows.length = 0
    for (let i = 0; i < n; i++) backend.historyRows.push({ id: `h-${i}` })
  }

  it('首屏 50，加载更多窗口 +50；返回不足一屏即到底', async () => {
    seedHistory(120)
    const store = useWorkspaceStore()
    backend.setActive('p-a')
    await store.init()

    await store.loadHistories()
    expect(backend.historyCalls.at(-1)).toBe(50)
    expect(store.histories).toHaveLength(50)
    expect(store.historyHasMore).toBe(true)

    await store.loadMoreHistories()
    expect(backend.historyCalls.at(-1)).toBe(100)
    expect(store.histories).toHaveLength(100)
    expect(store.historyHasMore).toBe(true)

    // 后端无 offset：窗口 150 但只剩 120 条 → 不足一屏，判定到底
    await store.loadMoreHistories()
    expect(backend.historyCalls.at(-1)).toBe(150)
    expect(store.histories).toHaveLength(120)
    expect(store.historyHasMore).toBe(false)
  })

  it('切换项目重置分页窗口（新项目从首屏 50 条重新拉）', async () => {
    seedHistory(120)
    const store = useWorkspaceStore()
    backend.setActive('p-a')
    await store.init()
    await store.loadHistories()
    await store.loadMoreHistories()
    expect(backend.historyCalls.at(-1)).toBe(100)

    await store.switchProject('p-b')
    await store.loadHistories()
    expect(backend.historyCalls.at(-1)).toBe(50)
    expect(store.historyHasMore).toBe(true)
  })

  it('加载失败只提示一次；成功加载后再次失败重新提示', async () => {
    seedHistory(10)
    const store = useWorkspaceStore()
    backend.setActive('p-a')
    await store.init()

    backend.setHistoryImpl(() => Promise.reject(new Error('backend down')))
    await store.loadHistories()
    await store.loadHistories()
    expect(toastMock.error).toHaveBeenCalledTimes(1)
    expect(toastMock.error.mock.calls[0]?.[0]).toBe('加载请求历史失败')

    backend.setHistoryImpl(null)
    await store.loadHistories()
    expect(toastMock.error).toHaveBeenCalledTimes(1)

    backend.setHistoryImpl(() => Promise.reject(new Error('down again')))
    await store.loadHistories()
    expect(toastMock.error).toHaveBeenCalledTimes(2)
  })
})
