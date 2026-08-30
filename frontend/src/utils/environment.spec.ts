import { describe, expect, it } from 'vitest'
import type { Environment } from '../types/foxApi'
import {
  defaultModule,
  effectiveVariable,
  envBaseUrl,
  envColorClass,
  environmentVariableMap,
  joinBaseUrl,
  moduleBaseUrl,
  moduleByName,
  normalizeBaseUrl,
  resolveRequestUrl,
  resolveVariables,
} from './environment'

function mkEnv(overrides: Partial<Environment> = {}): Environment {
  return {
    id: 'e1',
    name: '测试环境',
    modules: [],
    variables: [],
    created_at: '',
    updated_at: '',
    ...overrides,
  }
}

const multiModuleEnv = mkEnv({
  modules: [
    { id: 'm-pay', module_name: '支付', base_url: 'https://pay.example.com', is_default: true },
    { id: 'm-acq', module_name: '收单', base_url: 'https://acq.example.com', is_default: false },
    { id: 'm-api', module_name: 'api', base_url: 'http://dev-test01.redotpay.inet:8092', is_default: false },
  ],
})

describe('envColorClass', () => {
  it('按名称启发式归类（中英文大小写不敏感）', () => {
    expect(envColorClass('开发环境')).toBe('dev')
    expect(envColorClass('Development')).toBe('dev')
    expect(envColorClass('QA')).toBe('test')
    expect(envColorClass('Staging')).toBe('staging')
    expect(envColorClass('production')).toBe('prod')
    expect(envColorClass('全局')).toBe('global')
    expect(envColorClass('自定义')).toBe('')
  })
})

describe('defaultModule / moduleByName', () => {
  it('优先取 is_default，无标记取第一个', () => {
    expect(defaultModule(multiModuleEnv)?.module_name).toBe('支付')
    const noFlag = mkEnv({
      modules: [
        { id: 'a', module_name: 'A', base_url: 'https://a.com', is_default: false },
        { id: 'b', module_name: 'B', base_url: 'https://b.com', is_default: false },
      ],
    })
    expect(defaultModule(noFlag)?.module_name).toBe('A')
    expect(defaultModule(null)).toBeUndefined()
  })

  it('按 id 或 名称命中；无效键回退默认；空键取默认', () => {
    expect(moduleByName(multiModuleEnv, 'm-acq')?.module_name).toBe('收单')
    expect(moduleByName(multiModuleEnv, 'api')?.module_name).toBe('api')
    expect(moduleByName(multiModuleEnv, '')?.module_name).toBe('支付')
    expect(moduleByName(multiModuleEnv, '不存在')?.module_name).toBe('支付')
  })

  it('项目偏好：默认模块优先取当前项目绑定的模块（回归：开放演示误落 4010）', () => {
    // is_default 钉在用户服务（4010）上；开放演示有自己的模块
    const env = mkEnv({
      modules: [
        {
          id: 'm-users',
          project_id: 'proj-users',
          module_name: '小奏技术 · 用户服务',
          base_url: 'http://127.0.0.1:4010',
          is_default: true,
        },
        {
          id: 'm-open',
          project_id: 'proj-open',
          module_name: '小奏技术 · 开放演示',
          base_url: 'https://jsonplaceholder.typicode.com',
          is_default: false,
        },
      ],
    })
    expect(defaultModule(env, 'proj-open')?.module_name).toBe('小奏技术 · 开放演示')
    expect(defaultModule(env, 'proj-users')?.module_name).toBe('小奏技术 · 用户服务')
    // 无项目上下文 / 项目无绑定模块 → 回退 is_default
    expect(defaultModule(env)?.module_name).toBe('小奏技术 · 用户服务')
    expect(defaultModule(env, 'proj-不存在')?.module_name).toBe('小奏技术 · 用户服务')

    expect(moduleByName(env, '', 'proj-open')?.base_url).toBe('https://jsonplaceholder.typicode.com')
    expect(moduleBaseUrl(env, null, 'proj-open')).toBe('https://jsonplaceholder.typicode.com')
    expect(envBaseUrl(env, 'proj-open')).toBe('https://jsonplaceholder.typicode.com')
  })
})

describe('envBaseUrl / moduleBaseUrl', () => {
  it('取默认模块基址', () => {
    expect(envBaseUrl(multiModuleEnv)).toBe('https://pay.example.com')
    expect(moduleBaseUrl(multiModuleEnv, '收单')).toBe('https://acq.example.com')
    expect(moduleBaseUrl(multiModuleEnv)).toBe('https://pay.example.com')
    expect(envBaseUrl(null)).toBe('')
    expect(envBaseUrl(mkEnv())).toBe('')
  })

  it('无模块时回退 base_url 为名的已启用变量', () => {
    const env = mkEnv({
      variables: [
        { key: 'base_url', remote_value: ' https://x.com/ ', local_value: '', enabled: true, description: null },
      ],
    })
    expect(envBaseUrl(env)).toBe('https://x.com/')
  })

  it('禁用变量不参与回退', () => {
    const env = mkEnv({
      variables: [
        { key: 'base_url', remote_value: 'https://x.com', local_value: '', enabled: false, description: null },
      ],
    })
    expect(envBaseUrl(env)).toBe('')
  })
})

describe('effectiveVariable / environmentVariableMap', () => {
  it('本地值优先，其次远程值', () => {
    expect(effectiveVariable({ remote_value: 'r', local_value: 'l' })).toBe('l')
    expect(effectiveVariable({ remote_value: ' r ', local_value: '  ' })).toBe('r')
    expect(effectiveVariable({ remote_value: '', local_value: '' })).toBe('')
  })

  it('扁平注入表：enabled 才注入、本地优先、禁用跳过', () => {
    const env = mkEnv({
      modules: [
        { id: 'm1', module_name: '支付', base_url: 'https://pay.example.com', is_default: true },
      ],
      variables: [
        { key: 'token', remote_value: 'abc', local_value: 'LOCAL', enabled: true, description: null },
        { key: 'skipped', remote_value: 'x', local_value: '', enabled: false, description: null },
        { key: 'junk', remote_value: 'y', local_value: '', enabled: false, description: null },
      ],
    })
    const map = environmentVariableMap(env)
    expect(map.token).toBe('LOCAL')
    expect(map.skipped).toBeUndefined()
    // 默认模块基址自动注入 base_url
    expect(map.base_url).toBe('https://pay.example.com')
  })

  it('已显式定义 base_url 变量时不覆盖为默认模块', () => {
    const env = mkEnv({
      modules: [
        { id: 'm1', module_name: '支付', base_url: 'https://pay.example.com', is_default: true },
      ],
      variables: [
        { key: 'base_url', remote_value: 'https://override.example.com', local_value: '', enabled: true, description: null },
      ],
    })
    expect(environmentVariableMap(env).base_url).toBe('https://override.example.com')
  })
})

describe('normalizeBaseUrl', () => {
  it('去掉尾部斜杠但保留协议本身', () => {
    expect(normalizeBaseUrl('https://x.com/')).toBe('https://x.com')
    expect(normalizeBaseUrl('https://x.com///')).toBe('https://x.com')
    expect(normalizeBaseUrl('https://')).toBe('https://')
  })
})

describe('joinBaseUrl', () => {
  it('相对路径拼基址、去双斜杠、完整 URL 直用', () => {
    expect(joinBaseUrl('https://x.com', '/users')).toBe('https://x.com/users')
    expect(joinBaseUrl('https://x.com/', 'users')).toBe('https://x.com/users')
    expect(joinBaseUrl('', '/users')).toBe('/users')
    expect(joinBaseUrl('https://x.com/', 'https://full.example.com/a')).toBe('https://full.example.com/a')
  })
})

describe('resolveRequestUrl（请求拼接核心）', () => {
  it('显式模块命中：最终 URL = 模块基址 + 相对路径', () => {
    const r = resolveRequestUrl(multiModuleEnv, '收单', '/orders')
    expect(r).toMatchObject({ url: 'https://acq.example.com/orders', moduleName: '收单', fellBack: false })
  })

  it('按模块名命中（含端口示例）', () => {
    const r = resolveRequestUrl(multiModuleEnv, 'api', '/health')
    expect(r.url).toBe('http://dev-test01.redotpay.inet:8092/health')
  })

  it('未指定模块：降级默认模块', () => {
    const r = resolveRequestUrl(multiModuleEnv, null, '/users')
    expect(r).toMatchObject({ url: 'https://pay.example.com/users', moduleName: '支付', fellBack: false })
    const r2 = resolveRequestUrl(multiModuleEnv, '', '/users')
    expect(r2.url).toBe('https://pay.example.com/users')
  })

  it('无效模块键：回退默认模块并标记 fellBack', () => {
    const r = resolveRequestUrl(multiModuleEnv, '不存在', '/x')
    expect(r).toMatchObject({ url: 'https://pay.example.com/x', moduleName: '支付', fellBack: true })
  })

  it('项目偏好：未绑定模块的请求按所在项目解析默认模块', () => {
    const env = mkEnv({
      modules: [
        {
          id: 'm-users',
          project_id: 'proj-users',
          module_name: '小奏技术 · 用户服务',
          base_url: 'http://127.0.0.1:4010',
          is_default: true,
        },
        {
          id: 'm-open',
          project_id: 'proj-open',
          module_name: '小奏技术 · 开放演示',
          base_url: 'https://jsonplaceholder.typicode.com',
          is_default: false,
        },
      ],
    })
    const r = resolveRequestUrl(env, null, '/posts', {}, 'proj-open')
    expect(r.url).toBe('https://jsonplaceholder.typicode.com/posts')
    expect(r.moduleName).toBe('小奏技术 · 开放演示')
  })

  it('完整 URL 路径直用，不按模块拼接', () => {
    const r = resolveRequestUrl(multiModuleEnv, '支付', 'https://full.example.com/a',)
    expect(r.url).toBe('https://full.example.com/a')
    expect(r.moduleName).toBe('')
  })

  it('无模块环境：无基址仅返回路径', () => {
    const r = resolveRequestUrl(mkEnv(), '支付', '/users')
    expect(r).toMatchObject({ url: '/users', moduleName: '', fellBack: false })
  })

  it('基址中的 {{变量}} 以环境变量表解析', () => {
    const env = mkEnv({
      modules: [
        { id: 'm1', module_name: 'api', base_url: '{{host}}', is_default: true },
      ],
    })
    const withHost = mkEnv({
      modules: [
        { id: 'm1', module_name: 'api', base_url: '{{host}}', is_default: true },
      ],
      variables: [
        { key: 'host', remote_value: 'https://dev.example.com', local_value: '', enabled: true, description: null },
      ],
    })
    expect(resolveRequestUrl(env, null, '/ping').url).toBe('{{host}}/ping')
    const r = resolveRequestUrl(withHost, null, '/ping')
    expect(r.url).toBe('https://dev.example.com/ping')
  })

  it('调用方变量参与基址解析（extraVars）', () => {
    const env = mkEnv({
      modules: [{ id: 'm1', module_name: 'api', base_url: '{{host}}', is_default: true }],
    })
    const r = resolveRequestUrl(env, null, '/ping', { host: 'https://extra.example.com' })
    expect(r.url).toBe('https://extra.example.com/ping')
  })

  it('路径以斜杠开头不产生双斜杠', () => {
    const r = resolveRequestUrl(multiModuleEnv, '收单', '/')
    expect(r.url).toBe('https://acq.example.com/')
  })
})

describe('resolveVariables', () => {
  const vars = { base: '{{host}}/api', host: 'https://x.com', empty: '' }

  it('递归解析已知变量', () => {
    expect(resolveVariables('{{base}}/posts', vars)).toBe('https://x.com/api/posts')
  })

  it('未知或空值变量原样保留', () => {
    expect(resolveVariables('{{nope}}', vars)).toBe('{{nope}}')
    expect(resolveVariables('{{empty}}', vars)).toBe('{{empty}}')
  })

  it('循环引用在深度上限处停止', () => {
    const cyclic = { a: '{{b}}', b: '{{a}}' }
    const out = resolveVariables('{{a}}', cyclic)
    expect(out).toContain('{{')
  })
})