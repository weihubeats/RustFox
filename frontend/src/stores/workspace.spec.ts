/**
 * workspace store 多项目快照切换单测：
 * 项目 A 的草稿/标签在切到 B 再切回后必须原样恢复；B 首次进入为全新态。
 * useFoxApi 以模块级 mock 替换（内存假后端）。
 */
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import type { Endpoint, Environment, Project } from '../types/foxApi'

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
  return {
    projects,
    endpointsByProject,
    envsByProject,
    setActive: (id: string | null) => {
      activeId = id
    },
    active: () => activeId,
  }
})

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
    listRequestHistories: vi.fn().mockResolvedValue([]),
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
  useToast: () => ({ success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() }),
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
