/**
 * workspace store：工作区的单一状态源（Pinia）。
 *
 * 职责：
 * 1. 持有激活项目 + 文件夹/接口的扁平列表（树形由组件递归组装）；
 * 2. 标签页管理：openTabs（接口 id 有序集合）+ activeTabId + drafts（未保存草稿）；
 * 3. 树/标签/编辑器统一通过本 store 读写，避免跨组件手递 ref。
 *
 * 草稿语义（阶段 3 对齐 Dioxus 版）：打开标签时克隆一份 Endpoint 为草稿，
 * 编辑只改草稿；「保存」调用 save_endpoint 后回写列表并清除脏标记。
 */
import { defineStore } from 'pinia'
import { computed, nextTick, ref, watch } from 'vue'
import { useFoxApi } from '../composables/useFoxApi'
import { useToast } from '../composables/useToast'
import { planCrossGroupMove, planSameGroupMove, wouldCreateCycle } from './treeOps'
import { splitUrl } from '../utils/url'
import { envBaseUrl } from '../utils/environment'
import { applyCaseToRequest, restoreBody, snapshotRequest } from '../utils/testCases'
import type {
  AuthSpec,
  CurlParsed,
  Endpoint,
  Environment,
  EnvironmentVariable,
  ExecuteResponse,
  Folder,
  GlobalParam,
  HttpMethod,
  KeyValue,
  OAuth2Token,
  Project,
  RequestExample,
  RequestHistory,
  ResponseExample,
  TestCase,
  TestCaseCategory,
  TestCaseStatus,
} from '../types/foxApi'

/** 新建接口的默认请求规格（与 fox-core 模型字段一致）。 */
export function defaultRequestSpec(): Endpoint['request'] {
  return {
    params: [],
    headers: [],
    path_variables: [],
    auth: { type: 'none' },
    body: { mode: 'none' },
    active_tab: null,
    timeout_ms: null,
    follow_redirects: true,
    tests: null,
    disable_cookies: false,
  }
}

function eq(a: Endpoint, b: Endpoint): boolean {
  return JSON.stringify(a) === JSON.stringify(b)
}

export const useWorkspaceStore = defineStore('workspace', () => {
  const api = useFoxApi()
  const toast = useToast()

  const project = ref<Project | null>(null)
  const folders = ref<NonNullable<Awaited<ReturnType<typeof api.listFolders>>>[number][]>([])
  const endpoints = ref<Endpoint[]>([])
  const loadError = ref<string | null>(null)

  const openTabs = ref<string[]>([])
  const activeTabId = ref<string | null>(null)
  const drafts = ref<Map<string, Endpoint>>(new Map())

  /** 环境：列表 + 当前选中（execute_request 的 environment_id 来源）。 */
  const environments = ref<Environment[]>([])
  const activeEnvId = ref<string | null>(null)

  /** 全局变量（跨项目共享，优先级最低的 {{变量}} 兜底表）。 */
  const globalVariables = ref<EnvironmentVariable[]>([])

  /** 全局参数（每个请求自动注入的 query / header）。 */
  const globalParams = ref<GlobalParam[]>([])

  /** 会话级 Base URL（仅本次会话，不落库）；cURL 导入时自动预填为 URL 的 origin。 */
  const sessionBaseUrl = ref('http://localhost')

  /** 地址栏域名前缀（唯一真实数据源）：选中环境声明默认模块 base_url 或 base_url 变量时优先，否则回退会话 Base URL。
   *  默认模块随当前项目走（项目绑定的模块优先），多项目共用环境时各自落在自己的基址上。 */
  const urlDomain = computed(() => {
    const env = environments.value.find((e) => e.id === activeEnvId.value)
    const base = envBaseUrl(env, project.value?.id)
    if (base) return '{{base_url}}'
    return sessionBaseUrl.value || ''
  })

  /** 把当前选中环境的默认模块 base_url 更新为 url（地址栏粘贴完整 URL 时同步环境）。 */
  async function setEnvironmentBaseUrl(url: string): Promise<void> {
    const env = environments.value.find((e) => e.id === activeEnvId.value)
    if (!env) return
    const modules = [...env.modules]
    if (modules.length === 0) {
      modules.push({ id: crypto.randomUUID(), module_name: '默认', base_url: url, is_default: true })
    } else {
      // 优先写当前项目绑定的模块，其次 is_default，最后第一个
      const pid = project.value?.id
      let idx = pid ? modules.findIndex((m) => m.project_id === pid) : -1
      if (idx === -1) idx = modules.findIndex((m) => m.is_default)
      if (idx === -1) idx = 0
      modules[idx] = { ...modules[idx], base_url: url }
    }
    const updated: Environment = { ...env, modules }
    try {
      const saved = await api.saveEnvironment(updated)
      const idx = environments.value.findIndex((e) => e.id === env.id)
      if (idx !== -1) environments.value[idx] = saved
    } catch (err) {
      toast.error('更新环境变量失败', {
        message: err instanceof Error ? err.message : String(err),
      })
    }
  }

  /** 响应示例（按接口 id 缓存，openEndpoint 时懒加载）。 */
  const examples = ref<Map<string, ResponseExample[]>>(new Map())

  const activeEndpoint = computed(() => {
    if (!activeTabId.value) return null
    return drafts.value.get(activeTabId.value) ?? null
  })

  /**
   * 脏检查定版机制：
   * eq() 是全量 JSON.stringify 比较，而 isDirty 被树行 / 标签页高频调用——
   * 若直接暴露，草稿每敲一键就会触发所有可见节点各自重新序列化整个
   * Endpoint（O(可见节点 × 草稿体积) 的「stringify 风暴」）。
   * 这里用 deep watch + 微任务合并把连续编辑折叠为一次 dirtyTick 自增，
   * 同一定版内的重复调用命中 dirtyCache，序列化每键批次至多一次。
   */
  const dirtyTick = ref(0)
  let dirtyScheduled = false
  watch(
    drafts,
    () => {
      if (dirtyScheduled) return
      dirtyScheduled = true
      nextTick(() => {
        dirtyScheduled = false
        dirtyTick.value++
      })
    },
    { deep: true },
  )
  // 列表整体刷新（load/refresh）同样会改变「已保存态」，推进定版使缓存失效
  watch(endpoints, () => {
    dirtyTick.value++
  })

  /** [定版本号, 结果]：仅当本版未计算过时才做全量比较。 */
  const dirtyCache = new Map<string, [number, boolean]>()

  /**
   * 保存态索引：isDirty / titleOf 原来每次调用都 `endpoints.find`
   *（O(标签数 × 接口数)），此处随列表刷新重建一次，查询 O(1)。
   */
  const savedIndex = computed(() => new Map(endpoints.value.map((e) => [e.id, e] as const)))

  const isDirty = (id: string): boolean => {
    void dirtyTick.value // 建立响应式依赖：定版推进后调用方重新求值
    const draft = drafts.value.get(id)
    if (!draft) return false
    const saved = savedIndex.value.get(id)
    if (!saved) return true
    const cached = dirtyCache.get(id)
    if (cached && cached[0] === dirtyTick.value) return cached[1]
    const result = !eq(draft, saved)
    dirtyCache.set(id, [dirtyTick.value, result])
    return result
  }

  const draftOf = (id: string): Endpoint | null => drafts.value.get(id) ?? null

  /** 标签页标题：草稿名 > 保存名 > method+path。 */
  const titleOf = (id: string): string => {
    const d = drafts.value.get(id)
    if (d?.name) return d.name
    if (d) return `${d.method} ${d.path}`
    const saved = savedIndex.value.get(id)
    if (saved) return saved.name || `${saved.method} ${saved.path}`
    return '未保存'
  }

  /** 拉取文件夹 + 接口列表；knownProject 传入时跳过重复的 getActiveProject IPC。 */
  async function load(projectId: string, knownProject?: Project): Promise<void> {
    loadError.value = null
    try {
      const [p, f, e] = await Promise.all([
        knownProject ? Promise.resolve(knownProject) : api.getActiveProject(),
        api.listFolders(projectId),
        api.listEndpoints(projectId),
      ])
      project.value = p
      folders.value = f
      endpoints.value = e
    } catch (err) {
      loadError.value = err instanceof Error ? err.message : String(err)
      throw err
    }
  }

  /** 初始化：取激活项目（无则返回 null，调用方负责跳回项目列表）；同时恢复标签栏。 */
  async function init(): Promise<Project | null> {
    await ensureOpenProjectsRestored()
    const p = await api.getActiveProject()
    if (!p) return null
    project.value = p
    await load(p.id, p)
    await loadEnvironments()
    return p
  }

  /** 刷新文件夹 + 接口（树操作后调用）。 */
  async function refresh(): Promise<void> {
    if (!project.value) return
    const [f, e] = await Promise.all([
      api.listFolders(project.value.id),
      api.listEndpoints(project.value.id),
    ])
    folders.value = f
    endpoints.value = e
  }

  /** 切换到另一项目：当前项目 UI 态入快照，目标项目恢复快照或全新加载。
   *  草稿 / 打开的标签页 / 环境选择等在项目间来回切换均不丢失。 */
  async function switchProject(projectId: string): Promise<void> {
    if (project.value?.id === projectId) return
    // 应用可能从项目列表页直接进入：确保持久化的标签已恢复再操作
    await ensureOpenProjectsRestored()
    if (project.value) snapshots.set(project.value.id, snapshotCurrent())
    const p = await api.setActiveProject(projectId)
    if (!p) {
      // 目标项目已不存在（他处删除）：移除标签后抛错
      snapshots.delete(projectId)
      removeOpenTab(projectId)
      throw new Error('项目不存在或已删除')
    }
    await activateProject(p)
  }

  // ---------- 多项目标签：快照切换 ----------
  /** 顶部标签栏的项目（有序）。 */
  const openProjects = ref<{ id: string; name: string }[]>([])
  /** 标签列表持久化键（仅存 id 顺序；草稿等 UI 态不跨重启保留）。 */
  const OPEN_TABS_KEY = 'rustfox.open-projects'

  /** 非激活项目的完整 UI 快照。 */
  interface ProjectSnapshot {
    project: Project
    folders: Folder[]
    endpoints: Endpoint[]
    environments: Environment[]
    activeEnvId: string | null
    sessionBaseUrl: string
    openTabs: string[]
    drafts: Map<string, Endpoint>
    activeTabId: string | null
    activeView: 'debug' | 'design' | 'docs' | 'cases' | 'mock'
    examples: Map<string, ResponseExample[]>
    requestExamples: Map<string, RequestExample[]>
    testCases: Map<string, TestCase[]>
    histories: RequestHistory[]
    historyOnlyCurrent: boolean
  }
  /** 快照无需响应式：仅在切换瞬间读写。 */
  const snapshots = new Map<string, ProjectSnapshot>()

  function snapshotCurrent(): ProjectSnapshot {
    return {
      project: project.value!,
      folders: folders.value,
      endpoints: endpoints.value,
      environments: environments.value,
      activeEnvId: activeEnvId.value,
      sessionBaseUrl: sessionBaseUrl.value,
      openTabs: openTabs.value,
      drafts: drafts.value,
      activeTabId: activeTabId.value,
      activeView: activeView.value,
      examples: examples.value,
      requestExamples: requestExamples.value,
      testCases: testCases.value,
      histories: histories.value,
      historyOnlyCurrent: historyOnlyCurrent.value,
    }
  }

  function persistOpenProjects(): void {
    try {
      localStorage.setItem(OPEN_TABS_KEY, JSON.stringify(openProjects.value.map((t) => t.id)))
    } catch {
      // 存储不可用时静默：仅影响重启后的标签恢复
    }
  }

  function removeOpenTab(id: string): void {
    const idx = openProjects.value.findIndex((t) => t.id === id)
    if (idx !== -1) openProjects.value.splice(idx, 1)
    persistOpenProjects()
  }

  function upsertOpenTab(id: string, name: string): void {
    const idx = openProjects.value.findIndex((t) => t.id === id)
    if (idx === -1) openProjects.value.push({ id, name })
    else openProjects.value[idx].name = name
    persistOpenProjects()
  }

  // 重命名当前项目时同步标签栏名称
  watch(
    () => [project.value?.id, project.value?.name] as const,
    ([id, name]) => {
      if (id && name) upsertOpenTab(id, name)
    },
  )

  /** 激活目标项目：有快照则恢复（数据不重新拉取），否则全新加载。 */
  async function activateProject(p: Project): Promise<void> {
    const snap = snapshots.get(p.id)
    if (snap) {
      snapshots.delete(p.id)
      project.value = snap.project
      folders.value = snap.folders
      endpoints.value = snap.endpoints
      environments.value = snap.environments
      activeEnvId.value = snap.activeEnvId
      sessionBaseUrl.value = snap.sessionBaseUrl
      openTabs.value = snap.openTabs
      drafts.value = snap.drafts
      activeTabId.value = snap.activeTabId
      activeView.value = snap.activeView
      examples.value = snap.examples
      requestExamples.value = snap.requestExamples
      testCases.value = snap.testCases
      histories.value = snap.histories
      historyOnlyCurrent.value = snap.historyOnlyCurrent
      loadError.value = null
    } else {
      project.value = p
      openTabs.value = []
      drafts.value = new Map()
      examples.value = new Map()
      requestExamples.value = new Map()
      testCases.value = new Map()
      activeTabId.value = null
      activeView.value = 'debug'
      sessionBaseUrl.value = 'http://localhost'
      histories.value = []
      historyOnlyCurrent.value = false
      loadError.value = null
      await load(p.id, p)
      await loadEnvironments()
    }
    upsertOpenTab(p.id, p.name)
  }

  /** 关闭项目标签：丢弃快照；若是当前项目则切到相邻标签，一个不剩时清空（视图负责跳转）。 */
  function closeProjectTab(id: string): void {
    snapshots.delete(id)
    const idx = openProjects.value.findIndex((t) => t.id === id)
    if (idx !== -1) openProjects.value.splice(idx, 1)
    persistOpenProjects()
    if (project.value?.id !== id) return
    const next = openProjects.value[Math.min(idx, openProjects.value.length - 1)]
    if (next) {
      void switchProject(next.id).catch((err) => {
        toast.error('切换项目失败', { message: err instanceof Error ? err.message : String(err) })
      })
    } else {
      project.value = null
    }
  }

  /** 启动时恢复持久化的标签列表（名称从项目列表回填，已删除的项目自动剔除）。
   *  与已有条目做并集合并：恢复是异步的，期间用户可能已打开新标签。 */
  async function restoreOpenProjects(): Promise<void> {
    let ids: unknown
    try {
      ids = JSON.parse(localStorage.getItem(OPEN_TABS_KEY) ?? '[]')
    } catch {
      ids = []
    }
    if (Array.isArray(ids) && ids.length) {
      try {
        const all = await api.getProjects()
        const byId = new Map(all.map((p) => [p.id, p]))
        const restored = ids
          .filter((id): id is string => typeof id === 'string' && byId.has(id))
          .map((id) => ({ id, name: byId.get(id)!.name }))
        const seen = new Set(restored.map((t) => t.id))
        // 恢复期间用户已打开的标签排在恢复列表之后，避免丢失
        for (const t of openProjects.value) {
          if (!seen.has(t.id)) restored.push(t)
        }
        openProjects.value = restored
      } catch {
        // 项目列表拉取失败：保留现有标签，下次再试
      }
    }
    persistOpenProjects()
  }

  /** 恢复只做一次（store 生命周期内）；switchProject / init 前调用以确保标签就绪。 */
  let restorePromise: Promise<void> | null = null
  function ensureOpenProjectsRestored(): Promise<void> {
    restorePromise ??= restoreOpenProjects()
    return restorePromise
  }
  // store 创建即触发：应用可能直接落在项目列表页（此时 WorkspaceView 尚未挂载）
  void ensureOpenProjectsRestored()

  /** 打开接口为标签页；已打开则仅切换。草稿懒克隆自保存态。 */
  function openEndpoint(endpoint: Endpoint): void {
    if (!openTabs.value.includes(endpoint.id)) {
      openTabs.value.push(endpoint.id)
      drafts.value.set(endpoint.id, { ...endpoint, request: JSON.parse(JSON.stringify(endpoint.request)) })
    }
    activeTabId.value = endpoint.id
    // 缓存命中跳过：切 Tab 原来无条件触发三 IPC（示例/用例/测试用例各一次）。
    if (!examples.value.has(endpoint.id)) void loadExamples(endpoint.id)
    if (!requestExamples.value.has(endpoint.id)) void loadRequestExamples(endpoint.id)
    if (!testCases.value.has(endpoint.id)) void loadTestCases(endpoint.id)
  }

  /** 加载接口的响应示例（懒加载 + 缓存）。 */
  async function loadExamples(endpointId: string): Promise<void> {
    try {
      examples.value.set(endpointId, await api.listExamples(endpointId))
    } catch (err) {
      console.error('[workspace.loadExamples]', err)
      examples.value.set(endpointId, [])
    }
  }

  /** 把一次执行响应保存为示例（保存后刷新缓存）。 */
  async function saveAsExample(
    endpointId: string,
    name: string,
    response: ExecuteResponse,
  ): Promise<void> {
    const now = new Date().toISOString()
    const example = await api.saveExample({
      id: crypto.randomUUID(),
      endpoint_id: endpointId,
      name,
      status: response.status,
      headers: Object.fromEntries(response.headers),
      body: response.body,
      content_type: response.content_type,
      created_at: now,
      updated_at: now,
    })
    const list = examples.value.get(endpointId) ?? []
    const idx = list.findIndex((x) => x.id === example.id)
    if (idx === -1) list.unshift(example)
    else list[idx] = example
    examples.value.set(endpointId, list)
    toast.success(`示例已保存：${example.name}`)
  }

  async function removeExample(endpointId: string, exampleId: string): Promise<void> {
    await api.deleteExample(exampleId)
    const list = (examples.value.get(endpointId) ?? []).filter((x) => x.id !== exampleId)
    examples.value.set(endpointId, list)
  }

  /** 请求用例（按接口 id 缓存，openEndpoint 时懒加载）。 */
  const requestExamples = ref<Map<string, RequestExample[]>>(new Map())

  /** 加载接口的请求用例（懒加载 + 缓存，失败时按空处理）。 */
  async function loadRequestExamples(endpointId: string): Promise<void> {
    try {
      requestExamples.value.set(endpointId, await api.listRequestExamples(endpointId))
    } catch (err) {
      console.error('[workspace.loadRequestExamples]', err)
      requestExamples.value.set(endpointId, [])
    }
  }

  /** 把当前请求保存为用例快照（request 深拷贝落库，保存后置顶缓存）。 */
  async function saveRequestAsExample(
    endpointId: string,
    name: string,
    request: Endpoint['request'],
  ): Promise<boolean> {
    const trimmed = name.trim()
    if (!trimmed) {
      toast.warning('用例名称不能为空')
      return false
    }
    const now = new Date().toISOString()
    const example: RequestExample = {
      id: crypto.randomUUID(),
      endpoint_id: endpointId,
      name: trimmed,
      request: JSON.parse(JSON.stringify(request)),
      created_at: now,
      updated_at: now,
    }
    try {
      const saved = await api.saveRequestExample(example)
      const list = requestExamples.value.get(endpointId) ?? []
      list.unshift(saved)
      requestExamples.value.set(endpointId, list)
      toast.success(`请求用例已保存：${saved.name}`)
      return true
    } catch (err) {
      toast.error('保存请求用例失败', {
        message: err instanceof Error ? err.message : String(err),
      })
      return false
    }
  }

  /** 把请求用例回填到草稿（request 深拷贝 + 同步 active_tab 智能默认）。 */
  function applyRequestExample(endpointId: string, example: RequestExample): void {
    const draft = drafts.value.get(endpointId)
    if (!draft) return
    draft.request = JSON.parse(JSON.stringify(example.request)) as Endpoint['request']
  }

  async function deleteRequestExample(endpointId: string, exampleId: string): Promise<void> {
    try {
      await api.deleteRequestExample(exampleId)
    } catch (err) {
      toast.error('删除请求用例失败', {
        message: err instanceof Error ? err.message : String(err),
      })
      return
    }
    const list = (requestExamples.value.get(endpointId) ?? []).filter((x) => x.id !== exampleId)
    requestExamples.value.set(endpointId, list)
  }

  /** 接口页二级导航：调试 | 设计 | 文档预览 | 测试用例 | Mock。 */
  const activeView = ref<'debug' | 'design' | 'docs' | 'cases' | 'mock'>('debug')
  function setActiveView(view: 'debug' | 'design' | 'docs' | 'cases' | 'mock'): void {
    activeView.value = view
  }

  /** 测试用例（按接口 id 缓存，openEndpoint 时懒加载）。 */
  const testCases = ref<Map<string, TestCase[]>>(new Map())

  /** 最近一次运行结果元信息（状态码 / 耗时），驱动列表「运行结果」列与抽屉联动。 */
  const caseRunMeta = ref<Map<string, { status: number; durationMs: number }>>(new Map())

  /** 当前接口的用例数（导航 Tab 徽标 N）。 */
  const testCaseCount = computed(() => {
    const id = activeTabId.value
    return id ? (testCases.value.get(id)?.length ?? 0) : 0
  })

  /** 加载接口的测试用例（懒加载 + 缓存，失败时按空处理）。 */
  async function loadTestCases(endpointId: string): Promise<void> {
    try {
      testCases.value.set(endpointId, (await api.listTestCases(endpointId)) ?? [])
    } catch (err) {
      console.error('[workspace.loadTestCases]', err)
      testCases.value.set(endpointId, [])
    }
  }

  /** 保存测试用例（新建），返回是否成功。 */
  async function saveTestCase(
    endpointId: string,
    name: string,
    category: TestCaseCategory,
    request: Endpoint['request'],
    path: string,
    method: HttpMethod,
  ): Promise<boolean> {
    const trimmed = name.trim()
    if (!trimmed) {
      toast.warning('用例名称不能为空')
      return false
    }
    const snap = snapshotRequest(request)
    const testCase: TestCase = {
      id: crypto.randomUUID(),
      request_id: endpointId,
      name: trimmed,
      category,
      method,
      url_path: path,
      params: snap.params,
      headers: snap.headers,
      body_type: snap.body_type,
      body_content: snap.body_content,
      last_run_status: 'Untested',
      created_at: new Date().toISOString(),
    }
    try {
      const saved = await api.saveTestCase(testCase)
      const list = testCases.value.get(endpointId) ?? []
      list.push(saved)
      testCases.value.set(endpointId, list)
      toast.success(`测试用例已保存：${saved.name}`)
      return true
    } catch (err) {
      toast.error('保存测试用例失败', {
        message: err instanceof Error ? err.message : String(err),
      })
      return false
    }
  }

  /** 更新用例名称与分组（编辑）。 */
  async function renameTestCase(endpointId: string, caseId: string, name: string, category: TestCaseCategory): Promise<boolean> {
    const trimmed = name.trim()
    if (!trimmed) {
      toast.warning('用例名称不能为空')
      return false
    }
    try {
      await api.updateTestCaseMeta(caseId, trimmed, category)
      const list = testCases.value.get(endpointId)
      const target = list?.find((c) => c.id === caseId)
      if (target) {
        target.name = trimmed
        target.category = category
      }
      return true
    } catch (err) {
      toast.error('更新用例失败', {
        message: err instanceof Error ? err.message : String(err),
      })
      return false
    }
  }

  /** 克隆用例（另存为「名称 副本」，保留原快照）。 */
  async function cloneTestCase(endpointId: string, source: TestCase): Promise<boolean> {
    const testCase: TestCase = {
      ...JSON.parse(JSON.stringify(source)) as TestCase,
      id: crypto.randomUUID(),
      name: `${source.name} 副本`,
      last_run_status: 'Untested',
      created_at: new Date().toISOString(),
    }
    try {
      const saved = await api.saveTestCase(testCase)
      const list = testCases.value.get(endpointId) ?? []
      list.push(saved)
      testCases.value.set(endpointId, list)
      return true
    } catch (err) {
      toast.error('克隆用例失败', {
        message: err instanceof Error ? err.message : String(err),
      })
      return false
    }
  }

  /** 删除测试用例。 */
  async function removeTestCase(endpointId: string, caseId: string): Promise<void> {
    try {
      await api.deleteTestCase(caseId)
    } catch (err) {
      toast.error('删除用例失败', {
        message: err instanceof Error ? err.message : String(err),
      })
      return
    }
    const list = (testCases.value.get(endpointId) ?? []).filter((c) => c.id !== caseId)
    testCases.value.set(endpointId, list)
  }

  /** 用例快照 → 回填草稿（method / path / params / headers / body），不切换视图。 */
  function applyTestCaseToDraft(endpointId: string, testCase: TestCase): void {
    const draft = drafts.value.get(endpointId)
    if (!draft) return
    draft.method = testCase.method
    draft.path = testCase.url_path
    applyCaseToRequest(draft.request, testCase)
    if (!draft.request.active_tab) {
      draft.request.active_tab = ['POST', 'PUT', 'PATCH'].includes(testCase.method) ? 'body' : 'params'
    }
  }

  /** 「在调试页打开」：回填草稿并显式切换到调试页。 */
  function openTestCaseInDebug(endpointId: string, testCase: TestCase): void {
    applyTestCaseToDraft(endpointId, testCase)
    activeView.value = 'debug'
  }

  /** 用例快照 → 可发送请求（URL + spec），单跑与集合跑共用。 */
  function buildCaseRequest(testCase: TestCase): { url: string; spec: Endpoint['request'] } {
    const isAbs = /^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(testCase.url_path)
    const path = testCase.url_path.startsWith('/') ? testCase.url_path : `/${testCase.url_path}`
    return {
      url: isAbs ? testCase.url_path : `${urlDomain.value}${path}`,
      spec: {
        params: testCase.params,
        headers: testCase.headers,
        path_variables: [] as KeyValue[],
        auth: { type: 'none' } as AuthSpec,
        body: restoreBody(testCase.body_type, testCase.body_content),
        active_tab: null,
        timeout_ms: null,
        follow_redirects: true,
        tests: null,
      },
    }
  }

  /** 运行单个用例：拼 URL 执行请求，回写运行状态。返回响应或 null。 */
  async function runTestCase(endpointId: string, testCase: TestCase, environmentId: string | null): Promise<ExecuteResponse | null> {
    try {
      const { url, spec } = buildCaseRequest(testCase)
      const response = await api.executeRequest({
        url,
        method: testCase.method,
        spec,
        environment_id: environmentId,
        project_id: project.value?.id ?? null,
        endpoint_id: endpointId,
        request_id: null,
      })
      const status: TestCaseStatus = response.status >= 200 && response.status < 400 ? 'Success' : 'Failed'
      caseRunMeta.value.set(testCase.id, {
        status: response.status,
        durationMs: response.duration_ms,
      })
      void updateCaseStatusLocally(endpointId, testCase.id, status)
      return response
    } catch (err) {
      void updateCaseStatusLocally(endpointId, testCase.id, 'Failed')
      throw err
    }
  }

  /** 回写用例运行状态（本地 + 后端）。 */
  async function updateCaseStatusLocally(endpointId: string, caseId: string, status: TestCaseStatus): Promise<void> {
    const list = testCases.value.get(endpointId)
    const target = list?.find((c) => c.id === caseId)
    if (target) target.last_run_status = status
    try {
      await api.updateTestCaseStatus(caseId, status)
    } catch {
      /* 状态回写失败不影响主流程 */
    }
  }

  /** 抽屉「保存修改」：更新用例完整请求内容（本地 + 后端）。 */
  async function updateTestCaseContent(
    endpointId: string,
    caseId: string,
    payload: {
      method: HttpMethod
      urlPath: string
      params: KeyValue[]
      headers: KeyValue[]
      bodyType: string
      bodyContent: string
    },
  ): Promise<void> {
    await api.updateTestCaseContent(caseId, payload)
    const list = testCases.value.get(endpointId)
    const target = list?.find((c) => c.id === caseId)
    if (target) {
      target.method = payload.method
      target.url_path = payload.urlPath
      target.params = payload.params
      target.headers = payload.headers
      target.body_type = payload.bodyType
      target.body_content = payload.bodyContent
    }
  }

  /**
   * 运行接口的全部用例：一次 IPC 跑完整个集合（后端并发 5 + 可取消）。
   *
   * 原来前端串行 `for + await runTestCase`：N 个用例 = N 次 IPC ×（变量加载 +
   * 串行等待），且中途不可取消。结果与用例同序回填，可按序更新状态列。
   */
  async function runAllTestCases(
    endpointId: string,
    opts?: { runId?: string; onProgress?: (done: number, total: number) => void },
  ): Promise<{ total: number; success: number; cancelled: boolean }> {
    const cases = testCases.value.get(endpointId) ?? []
    if (cases.length === 0) return { total: 0, success: 0, cancelled: false }
    const endpoint = savedIndex.value.get(endpointId) ?? drafts.value.get(endpointId)
    const runId = opts?.runId ?? crypto.randomUUID()
    const result = await api.testCollection({
      items: cases.map((c) => {
        const { url, spec } = buildCaseRequest(c)
        return {
          endpoint: endpoint ?? {
            id: endpointId,
            project_id: project.value?.id ?? '',
            folder_id: null,
            name: c.name,
            method: c.method,
            path: c.url_path,
            description: '',
            status: 'designing',
            sort_order: 0,
            request: spec,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
          url,
          spec,
        }
      }),
      environment_id: activeEnvId.value,
      concurrency: 5,
      run_id: runId,
    })
    opts?.onProgress?.(result.results.length, cases.length)
    // 按序回填：results 与输入 cases 同序（取消时只含已完成项）。
    let success = 0
    result.results.forEach((r, i) => {
      const c = cases[i]
      if (!c) return
      const status: TestCaseStatus = r.ok ? 'Success' : 'Failed'
      if (r.ok) success += 1
      caseRunMeta.value.set(c.id, {
        status: r.status ?? 0,
        durationMs: r.duration_ms ?? 0,
      })
      void updateCaseStatusLocally(endpointId, c.id, status)
    })
    return { total: cases.length, success, cancelled: result.cancelled }
  }

  /** 取消在途的集合测试（runId 由调用方持有）。 */
  async function cancelAllTestCases(runId: string): Promise<boolean> {
    try {
      return await api.cancelTestCollection(runId)
    } catch {
      return false
    }
  }

  /** 打开「新建接口」草稿标签页（未持久化，保存时生成 id）；默认标题「未命名接口」，创建后自动聚焦全选便于输入。 */
  function openNewEndpoint(folderId: string | null): void {
    const now = new Date().toISOString()
    const blank: Endpoint = {
      id: crypto.randomUUID(),
      project_id: project.value?.id ?? '',
      folder_id: folderId,
      name: '未命名接口',
      method: 'GET',
      path: '/',
      description: '',
      status: 'designing',
      sort_order: 0,
      request: defaultRequestSpec(),
      created_at: now,
      updated_at: now,
    }
    drafts.value.set(blank.id, blank)
    if (!openTabs.value.includes(blank.id)) openTabs.value.push(blank.id)
    activeTabId.value = blank.id
    focusTitle()
  }

  /** 标题聚焦信号：新建接口时自增，编辑器监听并聚焦全选标题（TabBar「+」/ ⌘T / 树内新建共用）。 */
  const focusTitleSignal = ref(0)
  function focusTitle(): void {
    focusTitleSignal.value += 1
  }

  function setDraft(endpoint: Endpoint): void {
    drafts.value.set(endpoint.id, { ...endpoint })
  }

  function closeTab(id: string): void {
    const idx = openTabs.value.indexOf(id)
    if (idx === -1) return
    openTabs.value.splice(idx, 1)
    drafts.value.delete(id)
    // 示例缓存随标签释放（每条含完整响应 body），重开标签时懒加载重建。
    examples.value.delete(id)
    requestExamples.value.delete(id)
    testCases.value.delete(id)
    caseRunMeta.value.delete(id)
    if (activeTabId.value === id) {
      activeTabId.value = openTabs.value[idx] ?? openTabs.value[idx - 1] ?? null
    }
  }

  /** 保存当前草稿：新建（列表无此 id）走创建，否则走更新。 */
  async function saveActiveDraft(): Promise<boolean> {
    const draft = activeEndpoint.value
    if (!draft) return false
    if (!draft.name.trim()) {
      toast.warning('接口名称不能为空')
      return false
    }
    if (!draft.path.trim().startsWith('/')) {
      toast.warning('接口路径必须以 / 开头')
      return false
    }
    try {
      const saved = await api.saveEndpoint(draft)
      const savedClone = JSON.parse(JSON.stringify(saved)) as Endpoint
      const idx = endpoints.value.findIndex((e) => e.id === saved.id)
      if (idx === -1) {
        endpoints.value.push(savedClone)
      } else {
        endpoints.value[idx] = savedClone
      }
      drafts.value.set(saved.id, {
        ...saved,
        request: JSON.parse(JSON.stringify(saved.request)),
      })
      toast.success(`接口已保存：${saved.name}`)
      return true
    } catch (err) {
      toast.error('保存失败', { message: err instanceof Error ? err.message : String(err) })
      return false
    }
  }

  /**
   * 删除撤销（单级）：删除前快照对象，Toast 提供「撤销」动作。
   * 文件夹含整棵子树（子文件夹 + 其下全部接口），撤销时按原 id 恢复。
   */
  interface DeletedSnapshot {
    kind: 'endpoint' | 'folder'
    label: string
    folders: Folder[]
    endpoints: Endpoint[]
  }
  const lastDeleted = ref<DeletedSnapshot | null>(null)

  /** 撤销上一次删除（快照整体回写 + 刷新列表）。 */
  async function undoDelete(): Promise<boolean> {
    const snap = lastDeleted.value
    if (!snap) return false
    lastDeleted.value = null
    try {
      for (const f of snap.folders) {
        await api.saveFolder({ ...f })
      }
      for (const e of snap.endpoints) {
        await api.saveEndpoint({ ...e })
      }
      await refresh()
      toast.success(`已撤销删除：${snap.label}`)
      return true
    } catch (err) {
      toast.error('撤销删除失败', { message: err instanceof Error ? err.message : String(err) })
      return false
    }
  }

  function collectSubtree(folderId: string): { folders: Folder[]; endpoints: Endpoint[] } {
    const outFolders: Folder[] = []
    const outEndpoints: Endpoint[] = []
    const walk = (fid: string): void => {
      const f = folders.value.find((x) => x.id === fid)
      if (f) outFolders.push({ ...f })
      for (const e of endpoints.value.filter((e) => e.folder_id === fid)) {
        outEndpoints.push({ ...e })
      }
      for (const child of folders.value.filter((x) => x.parent_id === fid)) {
        walk(child.id)
      }
    }
    walk(folderId)
    return { folders: outFolders, endpoints: outEndpoints }
  }

  async function deleteEndpoint(endpointId: string): Promise<void> {
    const snapshot = endpoints.value.find((e) => e.id === endpointId)
    await api.deleteEndpoint(endpointId)
    closeTab(endpointId)
    await refresh()
    if (snapshot) {
      lastDeleted.value = {
        kind: 'endpoint',
        label: snapshot.name || snapshot.path,
        folders: [],
        endpoints: [{ ...snapshot }],
      }
      toast.info(`接口已删除：${snapshot.name || snapshot.path}`, {
        duration: 8000,
        action: { label: '撤销', run: () => void undoDelete() },
      })
    } else {
      toast.info('接口已删除')
    }
  }

  async function duplicateEndpoint(endpointId: string): Promise<void> {
    const dup = await api.duplicateEndpoint(endpointId)
    await refresh()
    openEndpoint(dup)
    toast.info(`已复制：${dup.name}`)
  }

  /** 加载环境列表（全局）+ 当前激活环境 + 全局变量 + 全局参数。 */
  async function loadEnvironments(): Promise<void> {
    const [envs, active, global, params] = await Promise.all([
      api.listEnvironments(),
      api.getActiveEnvironment(),
      api.getGlobalVariables(),
      api.getGlobalParams(),
    ])
    environments.value = envs
    activeEnvId.value = active?.id ?? null
    globalVariables.value = global
    globalParams.value = params
  }

  /** 保存全局变量并同步本地副本。 */
  async function saveGlobalVariables(variables: EnvironmentVariable[]): Promise<void> {
    await api.saveGlobalVariables(variables)
    globalVariables.value = variables
  }

  /** 保存全局参数并同步本地副本。 */
  async function saveGlobalParams(params: GlobalParam[]): Promise<void> {
    await api.saveGlobalParams(params)
    globalParams.value = params
  }

  /** 切换激活环境（null = 不使用环境）；环境须属于当前项目（后端校验）。 */
  async function setEnvironment(environmentId: string | null): Promise<void> {
    const env = await api.setActiveEnvironment(environmentId)
    activeEnvId.value = env?.id ?? null
  }

  /** 新建环境（全局维度；模块随项目自动同步）。 */
  async function createEnvironment(name: string): Promise<void> {
    const now = new Date().toISOString()
    const env = await api.saveEnvironment({
      id: crypto.randomUUID(),
      name,
      modules: [],
      variables: [],
      created_at: now,
      updated_at: now,
    })
    environments.value.push(env)
    toast.success(`环境已创建：${env.name}`)
  }

  /** 保存（upsert）环境并同步本地列表。 */
  async function updateEnvironment(
    env: Environment,
    opts?: { silent?: boolean },
  ): Promise<Environment> {
    const saved = await api.saveEnvironment({ ...env, updated_at: new Date().toISOString() })
    const idx = environments.value.findIndex((e) => e.id === saved.id)
    if (idx === -1) environments.value.push(saved)
    else environments.value[idx] = saved
    if (!opts?.silent) toast.success(`环境已保存：${saved.name}`)
    return saved
  }

  /** 删除环境；若删除的是当前激活环境则清空选中。 */
  async function deleteEnvironment(environmentId: string): Promise<void> {
    await api.deleteEnvironment(environmentId)
    environments.value = environments.value.filter((e) => e.id !== environmentId)
    if (activeEnvId.value === environmentId) activeEnvId.value = null
    toast.success('环境已删除')
  }

  /**
   * 导入接口落地：按 folder_hint 复用/新建文件夹，保存接口并附带示例。
   *
   * 原来全串行（N 接口 ×（1 文件夹 + 1 接口 + E 示例）次 IPC 顺序等待）；
   * 现在限并发 4 的 worker 池并行落库，sort_order 预分配保证顺序语义，
   * 结束时一次 refresh 对齐列表（避免中途多次重渲染）。
   */
  async function importEndpoints(
    items: Array<{ name: string; method: string; path: string; description?: string; request: Endpoint['request']; examples?: Array<{ name: string; status: number; content_type: string; headers: Record<string, string>; body: string }>; folder_hint?: string | null }>,
  ): Promise<{ endpoints: number; examples: number }> {
    if (!project.value) return { endpoints: 0, examples: 0 }
    const projectId = project.value.id
    const now = new Date().toISOString()
    const baseSort = endpoints.value.length
    const baseFolderSort = folders.value.length
    // folder_hint → id（预扫本地 + 本次新建，避免并发重复建同名文件夹）。
    const folderCache = new Map<string, string>()
    for (const f of folders.value) folderCache.set(`\0${f.name}`, f.id)
    let folderSeq = 0
    const folderMutex = { locked: false }

    async function resolveFolder(hint: string | null | undefined): Promise<string | null> {
      const name = hint?.trim()
      if (!name) return null
      const key = `\0${name}`
      const hit = folderCache.get(key)
      if (hit) return hit
      // 建文件夹串行化（同名并发只建一次；文件夹数量少，串行无压力）。
      while (folderMutex.locked) await new Promise((r) => setTimeout(r, 5))
      folderMutex.locked = true
      try {
        const recheck = folderCache.get(key)
        if (recheck) return recheck
        const folder = await api.saveFolder({
          id: crypto.randomUUID(),
          project_id: projectId,
          parent_id: null,
          name,
          sort_order: baseFolderSort + folderSeq++,
          created_at: now,
          updated_at: now,
        })
        folderCache.set(key, folder.id)
        return folder.id
      } finally {
        folderMutex.locked = false
      }
    }

    let exampleCount = 0
    async function saveOne(item: (typeof items)[number], index: number): Promise<void> {
      const folderId = await resolveFolder(item.folder_hint)
      const endpoint = await api.saveEndpoint({
        id: crypto.randomUUID(),
        project_id: projectId,
        folder_id: folderId,
        name: item.name,
        method: item.method as Endpoint['method'],
        path: item.path,
        description: item.description ?? '',
        request: item.request,
        sort_order: baseSort + index,
        status: 'designing',
        created_at: now,
        updated_at: now,
      })
      for (const ex of item.examples ?? []) {
        await api.saveExample({
          id: crypto.randomUUID(),
          endpoint_id: endpoint.id,
          name: ex.name,
          status: ex.status,
          headers: ex.headers ?? {},
          body: ex.body,
          content_type: ex.content_type,
          created_at: now,
          updated_at: now,
        })
        exampleCount++
      }
    }

    // 限并发 worker 池（4 路 IPC 并行，大导入不再线性等待）。
    const CONCURRENCY = 4
    let next = 0
    await Promise.all(
      Array.from({ length: Math.min(CONCURRENCY, items.length) }, async () => {
        for (;;) {
          const i = next++
          if (i >= items.length) return
          await saveOne(items[i], i)
        }
      }),
    )
    await refresh()
    return { endpoints: items.length, examples: exampleCount }
  }

  /**
   * 发送草稿请求（url 为拼接后的完整地址；环境变量由后端按 environment_id 注入）。
   * 提供 requestId 后该请求可被「取消」（后端中止连接并返回 CANCELLED）。
   *
   * 前端超时联动：后端超时是最后兜底；前端到时主动 cancel，避免弱网下
   * 请求挂起无反馈。超时未配置时不设 timer（行为与原来一致）。
   */
  async function send(
    endpoint: Endpoint,
    url: string,
    requestId?: string,
    timeoutMs?: number | null,
  ): Promise<ExecuteResponse> {
    let spec = endpoint.request
    const auth = spec.auth as AuthSpec | undefined
    if (auth?.type === 'oauth2' && (auth.auth_url?.trim() || auth.token_url?.trim())) {
      const token = await api.oauthAccessToken(auth)
      const base = (auth.token ?? {}) as Partial<Pick<OAuth2Token, 'token_type' | 'refresh_token' | 'expires_at'>>
      spec = { ...spec, auth: { ...auth, token: { ...base, access_token: token } as OAuth2Token } }
    }
    const rid = requestId ?? crypto.randomUUID()
    const ms = timeoutMs ?? endpoint.request.timeout_ms ?? null
    let timer: ReturnType<typeof setTimeout> | undefined
    let timedOut = false
    if (ms && ms > 0) {
      timer = setTimeout(() => {
        timedOut = true
        void api.cancelRequest(rid)
      }, ms)
    }
    try {
      return await api.executeRequest({
        url,
        method: endpoint.method,
        spec,
        environment_id: activeEnvId.value,
        project_id: project.value?.id ?? null,
        endpoint_id: endpoint.id,
        request_id: rid,
      })
    } catch (err) {
      if (timedOut) {
        const e = err as Error & { code?: string }
        if (e?.code === 'CANCELLED') throw new Error(`请求超时（>${ms}ms），已自动取消`)
      }
      throw err
    } finally {
      if (timer) clearTimeout(timer)
    }
  }

  /** 树内重命名接口：保存 + 同步列表与打开中的草稿。 */
  async function renameEndpoint(endpointId: string, name: string): Promise<void> {
    const e = endpoints.value.find((x) => x.id === endpointId)
    if (!e) return
    const saved = await api.saveEndpoint({ ...e, name, updated_at: new Date().toISOString() })
    const idx = endpoints.value.findIndex((x) => x.id === endpointId)
    if (idx !== -1) endpoints.value[idx] = saved
    const draft = drafts.value.get(endpointId)
    if (draft) drafts.value.set(endpointId, { ...draft, name })
  }

  /** 将文件夹移动到 newParentId（null=根）的 targetIndex 处：重排相关兄弟组并落库。 */
  async function moveFolder(folderId: string, newParentId: string | null, targetIndex: number): Promise<void> {
    const moved = folders.value.find((f) => f.id === folderId)
    if (!moved) return
    // 防环：不允许把文件夹移入自身或其子孙（避免 parent_id 成环）
    if (wouldCreateCycle(folders.value, folderId, newParentId)) return
    let changed: Map<string, number>
    if (moved.parent_id === newParentId) {
      changed = planSameGroupMove(
        folders.value.filter((f) => f.parent_id === newParentId),
        folderId,
        targetIndex,
      )
    } else {
      changed = planCrossGroupMove(
        folders.value.filter((f) => f.parent_id === moved.parent_id),
        folders.value.filter((f) => f.parent_id === newParentId),
        folderId,
        targetIndex,
      )
      moved.parent_id = newParentId
    }
    if (changed.has(folderId)) moved.sort_order = changed.get(folderId)!
    await Promise.all(
      [...changed.keys()].map((id) => {
        const f = folders.value.find((x) => x.id === id)
        if (!f) return Promise.resolve()
        if (id === folderId) return api.saveFolder({ ...f })
        return api.saveFolder({ ...f, sort_order: changed.get(id)! })
      }),
    )
    await refresh()
  }

  /** 将接口移动到 folderId（null=根）的 targetIndex 处：重排相关兄弟组并落库。 */
  async function moveEndpoint(endpointId: string, newFolderId: string | null, targetIndex: number): Promise<void> {
    const moved = endpoints.value.find((e) => e.id === endpointId)
    if (!moved) return
    let changed: Map<string, number>
    if (moved.folder_id === newFolderId) {
      changed = planSameGroupMove(
        endpoints.value.filter((e) => e.folder_id === newFolderId),
        endpointId,
        targetIndex,
      )
    } else {
      changed = planCrossGroupMove(
        endpoints.value.filter((e) => e.folder_id === moved.folder_id),
        endpoints.value.filter((e) => e.folder_id === newFolderId),
        endpointId,
        targetIndex,
      )
      moved.folder_id = newFolderId
    }
    if (changed.has(endpointId)) moved.sort_order = changed.get(endpointId)!
    // 同步打开中的草稿（文件夹归属 / 顺序）：draft 是 openEndpoint 时的快照，
    // 若不更新，保存草稿（saveActiveDraft 写全量）会用旧 folder_id 覆盖移动，
    // 造成「移动到 B 文件夹，保存后又回到 A」。
    for (const [id, order] of changed) {
      const draft = drafts.value.get(id)
      if (!draft) continue
      draft.sort_order = order
      if (id === endpointId) draft.folder_id = moved.folder_id
    }
    await Promise.all(
      [...changed.keys()].map((id) => {
        const e = endpoints.value.find((x) => x.id === id)
        if (!e) return Promise.resolve()
        if (id === endpointId) return api.saveEndpoint({ ...e })
        return api.saveEndpoint({ ...e, sort_order: changed.get(id)! })
      }),
    )
    await refresh()
  }

  async function saveFolder(folder: FolderInput): Promise<void> {
    await api.saveFolder(folder)
    await refresh()
  }

  /**
   * 批量删除：接口 + 文件夹混合选中。
   * 归一化：选中文件夹子树内的接口/子文件夹不再单独处理（随文件夹级联）；
   * 快照整体可经 `undoDelete` 一次撤销。
   */
  async function batchDelete(epIds: string[], folderIds: string[]): Promise<void> {
    const folderById = new Map(folders.value.map((f) => [f.id, f]))
    const selectedFolders = new Set(folderIds.filter((id) => folderById.has(id)))
    // 去掉被祖先选中的文件夹（随祖先级联删除）。
    const isUnderSelected = (id: string): boolean => {
      let cur = folderById.get(id)?.parent_id ?? null
      while (cur) {
        if (selectedFolders.has(cur)) return true
        cur = folderById.get(cur)?.parent_id ?? null
      }
      return false
    }
    const topFolders = [...selectedFolders].filter((id) => !isUnderSelected(id))
    const topSet = new Set(topFolders)
    // 文件夹子树覆盖的接口（随文件夹级联，不再单独删除）。
    const coveredEps = new Set<string>()
    for (const e of endpoints.value) {
      let cur = e.folder_id
      while (cur) {
        if (topSet.has(cur)) {
          coveredEps.add(e.id)
          break
        }
        cur = folderById.get(cur)?.parent_id ?? null
      }
    }
    const directEps = epIds.filter(
      (id) => !coveredEps.has(id) && endpoints.value.some((e) => e.id === id),
    )
    if (!topFolders.length && !directEps.length) return

    const snapFolders = topFolders.flatMap((id) => collectSubtree(id).folders)
    const snapEndpoints = [
      ...topFolders.flatMap((id) => collectSubtree(id).endpoints),
      ...directEps.flatMap((id) => {
        const e = endpoints.value.find((x) => x.id === id)
        return e ? [{ ...e }] : []
      }),
    ]
    for (const id of topFolders) {
      for (const e of endpoints.value.filter((x) => {
        let cur: string | null = x.folder_id
        while (cur) {
          if (cur === id) return true
          cur = folderById.get(cur)?.parent_id ?? null
        }
        return false
      })) {
        closeTab(e.id)
      }
      await api.deleteFolder(id)
    }
    for (const id of directEps) {
      await api.deleteEndpoint(id)
      closeTab(id)
    }
    await refresh()
    const total = snapFolders.length + snapEndpoints.length
    lastDeleted.value = {
      kind: topFolders.length ? 'folder' : 'endpoint',
      label: `${total} 项`,
      folders: snapFolders,
      endpoints: snapEndpoints,
    }
    toast.info(`已删除 ${total} 项`, {
      duration: 8000,
      action: { label: '撤销', run: () => void undoDelete() },
    })
  }

  /** 批量移动接口到目标文件夹（末尾追加，单次刷新）。 */
  async function batchMoveEndpoints(ids: string[], folderId: string | null): Promise<void> {
    const list = endpoints.value.filter((e) => ids.includes(e.id))
    if (!list.length) return
    const siblings = endpoints.value.filter((e) => e.folder_id === folderId)
    let order = siblings.reduce((m, e) => Math.max(m, e.sort_order), -1) + 1
    await Promise.all(
      list.map((e) => api.saveEndpoint({ ...e, folder_id: folderId, sort_order: order++ })),
    )
    await refresh()
    toast.success(`已移动 ${list.length} 个接口`)
  }

  async function deleteFolder(folderId: string): Promise<void> {
    const folder = folders.value.find((f) => f.id === folderId)
    const snap = collectSubtree(folderId)
    for (const e of snap.endpoints) closeTab(e.id)
    await api.deleteFolder(folderId)
    await refresh()
    if (folder && (snap.folders.length || snap.endpoints.length)) {
      lastDeleted.value = {
        kind: 'folder',
        label: folder.name,
        folders: snap.folders,
        endpoints: snap.endpoints,
      }
      toast.info(`文件夹已删除：${folder.name}（含 ${snap.endpoints.length} 个接口）`, {
        duration: 8000,
        action: { label: '撤销', run: () => void undoDelete() },
      })
    } else {
      toast.info('文件夹已删除（含子项）')
    }
  }

  /** cURL 导入：打开为默认标题「未命名接口」的草稿（不落库），保存时生成 id；会话 Base URL 预填为 URL origin。 */
  function openCurlDraft(parsed: CurlParsed, folderId: string | null): void {
    const { path, params, origin } = splitUrl(parsed.url)
    const now = new Date().toISOString()
    const blank: Endpoint = {
      id: crypto.randomUUID(),
      project_id: project.value?.id ?? '',
      folder_id: folderId,
      name: '未命名接口',
      method: parsed.method,
      path,
      description: '',
      status: 'designing',
      sort_order: 0,
      request: {
        params,
        headers: parsed.headers,
        path_variables: [],
        auth: parsed.auth,
        body: parsed.body ?? { mode: 'none' },
        timeout_ms: null,
        follow_redirects: true,
        tests: null,
      },
      created_at: now,
      updated_at: now,
    }
    sessionBaseUrl.value = origin
    drafts.value.set(blank.id, blank)
    if (!openTabs.value.includes(blank.id)) openTabs.value.push(blank.id)
    activeTabId.value = blank.id
  }

  // ---------- 请求历史（侧栏「请求历史」页签；发送成功后由编辑器触发刷新） ----------
  const histories = ref<RequestHistory[]>([])
  /** 「仅当前接口」过滤（HistoryPanel 复选框；变更后需重新 loadHistories）。 */
  const historyOnlyCurrent = ref(false)

  async function loadHistories(): Promise<void> {
    if (!project.value) return
    const endpointId = historyOnlyCurrent.value ? activeEndpoint.value?.id ?? null : null
    try {
      histories.value = (await api.listRequestHistories(project.value.id, 50, endpointId)) ?? []
    } catch {
      // 历史为辅助数据，加载失败静默（避免干扰主流程）
    }
  }

  async function clearHistories(): Promise<void> {
    if (!project.value) return
    const endpointId = historyOnlyCurrent.value ? activeEndpoint.value?.id ?? null : null
    try {
      const removed = await api.clearRequestHistories(project.value.id, endpointId)
      histories.value = []
      toast.success(`已清空 ${removed} 条请求历史`)
    } catch (err) {
      toast.error('清空历史失败', {
        message: err instanceof Error ? err.message : String(err),
      })
    }
  }

  /** 历史摘要（request_summary_json）中可恢复的字段。 */
  interface HistorySummary {
    method?: HttpMethod
    url?: string
    spec?: Partial<Endpoint['request']>
  }

  /**
   * 点击历史记录 → 恢复到主编辑器。
   * - 归属接口存在：打开其草稿标签页并回填；不存在（临时请求）：新建「未命名接口」草稿；
   * - 回填 method / path / params / headers / body（摘要值为变量渲染后的实际发送值）；
   * - 认证保留接口自身配置（摘要入库时后端已置空）；URL origin 预填会话 Base URL。
   */
  function restoreFromHistory(h: RequestHistory): void {
    let summary: HistorySummary = {}
    try {
      summary = JSON.parse(h.request_summary_json) as HistorySummary
    } catch {
      // 旧版记录只有 method/url，按降级路径恢复
    }
    const url = summary.url ?? h.url
    const { path, params, origin } = splitUrl(url)

    const target = h.endpoint_id ? endpoints.value.find((e) => e.id === h.endpoint_id) : null
    let id: string
    if (target) {
      openEndpoint(target)
      id = target.id
    } else {
      id = crypto.randomUUID()
      const now = new Date().toISOString()
      drafts.value.set(id, {
        id,
        project_id: project.value?.id ?? '',
        folder_id: null,
        name: '未命名接口',
        method: (summary.method ?? h.method) as HttpMethod,
        path,
        description: '',
        status: 'designing',
        sort_order: 0,
        request: defaultRequestSpec(),
        created_at: now,
        updated_at: now,
      })
      if (!openTabs.value.includes(id)) openTabs.value.push(id)
      activeTabId.value = id
    }
    const draft = drafts.value.get(id)
    if (!draft) return

    draft.method = (summary.method ?? h.method) as HttpMethod
    draft.path = path
    if (origin) sessionBaseUrl.value = origin
    const spec = summary.spec
    draft.request.params = spec?.params?.length ? spec.params : params
    if (spec?.headers) draft.request.headers = spec.headers
    if (spec?.body) draft.request.body = spec.body
    if (spec?.path_variables?.length) draft.request.path_variables = spec.path_variables
    toast.info('已恢复该次请求到编辑器')
  }

  return {
    project,
    folders,
    endpoints,
    environments,
    activeEnvId,
    globalVariables,
    globalParams,
    sessionBaseUrl,
    urlDomain,
    setEnvironmentBaseUrl,
    loadError,
    openTabs,
    activeTabId,
    activeEndpoint,
    isDirty,
    draftOf,
    titleOf,
    init,
    load,
    refresh,
    switchProject,
    openProjects,
    closeProjectTab,
    openEndpoint,
    openNewEndpoint,
    focusTitleSignal,
    setDraft,
    closeTab,
    saveActiveDraft,
    deleteEndpoint,
    undoDelete,
    batchDelete,
    batchMoveEndpoints,
    duplicateEndpoint,
    renameEndpoint,
    moveFolder,
    moveEndpoint,
    saveFolder,
    deleteFolder,
    openCurlDraft,
    importEndpoints,
    send,
    loadEnvironments,
    setEnvironment,
    createEnvironment,
    updateEnvironment,
    deleteEnvironment,
    saveGlobalVariables,
    saveGlobalParams,
    examples,
    loadExamples,
    saveAsExample,
    removeExample,
    requestExamples,
    loadRequestExamples,
    saveRequestAsExample,
    applyRequestExample,
    deleteRequestExample,
    activeView,
    setActiveView,
    testCases,
    caseRunMeta,
    testCaseCount,
    loadTestCases,
    saveTestCase,
    renameTestCase,
    cloneTestCase,
    removeTestCase,
    applyTestCaseToDraft,
    openTestCaseInDebug,
    updateTestCaseContent,
    runTestCase,
    runAllTestCases,
    cancelAllTestCases,
    histories,
    historyOnlyCurrent,
    loadHistories,
    clearHistories,
    restoreFromHistory,
  }
})

import type { Folder as FolderInput } from '../types/foxApi'
