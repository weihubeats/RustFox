/**
 * BodyPanel 单测：raw JSON 编辑 + 格式化的数据流（回归：格式化使用了修改前的旧内容）。
 */
import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { reactive } from 'vue'
import BodyPanel from './BodyPanel.vue'
import { makeDraft } from '../testUtils/draftFixture'
import type { Endpoint } from '../types/foxApi'

function jsonDraft(raw: string): Endpoint {
  const d = makeDraft()
  d.request.body = { mode: 'json', raw }
  return reactive(d)
}

/** 读取草稿 json body 的 raw（联合类型窄化辅助）。 */
function rawOf(d: Endpoint): string {
  return (d.request.body as { raw: string }).raw
}

function mountPanel(draft: Endpoint) {
  return mount(BodyPanel, { props: { draft } })
}

/** 美化 / 压缩按钮已是纯图标（title 提示），按 title 定位。 */
function prettyBtnOf(wrapper: ReturnType<typeof mountPanel>) {
  return wrapper.findAll('button').find((b) => b.attributes('title') === '美化（格式化 JSON）')
}
function compactBtnOf(wrapper: ReturnType<typeof mountPanel>) {
  return wrapper.findAll('button').find((b) => b.attributes('title') === '压缩 JSON')
}

describe('BodyPanel：raw JSON 编辑与格式化', () => {
  it('编辑后点击「格式化」应格式化最新内容', async () => {
    const draft = jsonDraft('{"a":1}')
    const wrapper = mountPanel(draft)

    // raw + JSON 子类型 → 渲染 JsonEditor（工具条含「格式化」按钮）
    const formatBtn = prettyBtnOf(wrapper)
    expect(formatBtn).toBeDefined()
    expect(wrapper.findComponent({ name: 'JsonEditor' }).exists()).toBe(true)

    // 模拟用户编辑 textarea（input → v-model 写回草稿）
    const ta = wrapper.find('textarea')
    await ta.setValue('{"name":"alice","age":18}')
    expect(rawOf(draft)).toBe('{"name":"alice","age":18}')

    await formatBtn!.trigger('click')
    expect(rawOf(draft)).toBe(
      JSON.stringify({ name: 'alice', age: 18 }, null, 2),
    )
  })

  it('「美化」使用编辑后的最新内容', async () => {
    const draft = jsonDraft('{ "a": 1 }')
    const wrapper = mountPanel(draft)
    const ta = wrapper.find('textarea')
    await ta.setValue('{ "b": 2, "c": 3 }')
    await prettyBtnOf(wrapper)!.trigger('click')
    expect(rawOf(draft)).toBe('{\n  "b": 2,\n  "c": 3\n}')
  })

  it('「压缩」按钮输出紧凑 JSON；顶部工具栏显示校验状态', async () => {
    const draft = jsonDraft('{ "a": 1 }')
    const wrapper = mountPanel(draft)
    const ta = wrapper.find('textarea')
    await ta.setValue('{ "b": 2, "c": 3 }')
    await compactBtnOf(wrapper)!.trigger('click')
    expect(rawOf(draft)).toBe('{"b":2,"c":3}')

    // 顶部工具栏状态 Tag：有效（非悬浮层）
    expect(wrapper.find('.hl-float').exists()).toBe(false)
    expect(wrapper.find('.je-status').text()).toContain('JSON 有效')
  })

  it('回归：格式化保留重复键（多个 "body" 键不丢失）', async () => {
    const draft = jsonDraft(
      '{"title":"测试标题","body":"测试内容","body":"测试内容","body":"测试内容","body":"测试内容","userId":1}',
    )
    const wrapper = mountPanel(draft)
    await prettyBtnOf(wrapper)!.trigger('click')
    expect(rawOf(draft)).toBe(`{
  "title": "测试标题",
  "body": "测试内容",
  "body": "测试内容",
  "body": "测试内容",
  "body": "测试内容",
  "userId": 1
}`)
  })

  it('回归：DOM 已更新但 input 事件丢失时，格式化仍以 textarea 实际内容为准', async () => {
    const draft = jsonDraft('{"a":1}')
    const wrapper = mountPanel(draft)
    const ta = wrapper.find('textarea')

    // 直接改 DOM 值、不派发 input 事件（模拟真实浏览器中事件丢失/延迟的失同步），
    // 此时 props.modelValue 仍是旧值 {"a":1}
    ta.element.value = '{"name":"alice","age":18}'
    await prettyBtnOf(wrapper)!.trigger('click')

    expect(rawOf(draft)).toBe(
      JSON.stringify({ name: 'alice', age: 18 }, null, 2),
    )
  })

  it('在请求体中查找：打开 FindBar 并高亮与计数', async () => {
    const draft = jsonDraft('{"user":"fox","name":"fox"}')
    const wrapper = mountPanel(draft)

    // 点击搜索图标按钮打开 FindBar
    const searchBtn = wrapper.find('.bp-icon-btn')
    expect(searchBtn.exists()).toBe(true)
    await searchBtn.trigger('click')

    expect(wrapper.findComponent({ name: 'FindBar' }).exists()).toBe(true)

    // 输入查找词
    const findInput = wrapper.find('.findbar-input')
    await findInput.setValue('fox')

    // 等待防抖
    await new Promise((r) => setTimeout(r, 200))
    await wrapper.vm.$nextTick()

    // JsonEditor 渲染 mark 高亮
    expect(wrapper.findAll('.rp-find-mark').length).toBe(2)
    expect(wrapper.findAll('.rp-find-mark.active').length).toBe(1)

    // 切换下一个匹配
    await wrapper.findAll('.findbar-btn')[1].trigger('click')
    await wrapper.vm.$nextTick()
    expect(wrapper.find('.findbar-count').text()).toContain('2 / 2')

    // Esc 关闭
    await findInput.trigger('keydown', { key: 'Escape' })
    expect(wrapper.findComponent({ name: 'FindBar' }).exists()).toBe(false)
  })

  it('form-data→无→form-data：字段记忆还原', async () => {
    const draft = makeDraft({ id: 'ep-form-mem' })
    draft.request.body = {
      mode: 'multipart',
      fields: [{ key: 'a', value_type: 'text', value: '1', enabled: true }],
    }
    const wrapper = mountPanel(reactive(draft))
    const tabBtn = (label: string) =>
      wrapper.findAll('.seg-item').find((b) => b.text() === label)!

    await tabBtn('无').trigger('click')
    expect(draft.request.body.mode).toBe('none')

    await tabBtn('form-data').trigger('click')
    expect(draft.request.body).toEqual({
      mode: 'multipart',
      fields: [{ key: 'a', value_type: 'text', value: '1', enabled: true }],
    })
  })

  it('graphql→无→GraphQL：query 记忆还原', async () => {
    const draft = makeDraft({ id: 'ep-gql-mem' })
    draft.request.body = {
      mode: 'graphql',
      spec: { query: 'query { hero }', variables: '{}', operation_name: '' },
    }
    const wrapper = mountPanel(reactive(draft))
    const tabBtn = (label: string) =>
      wrapper.findAll('.seg-item').find((b) => b.text() === label)!

    await tabBtn('无').trigger('click')
    expect(draft.request.body.mode).toBe('none')

    await tabBtn('GraphQL').trigger('click')
    expect(draft.request.body).toEqual({
      mode: 'graphql',
      spec: { query: 'query { hero }', variables: '{}', operation_name: '' },
    })
  })

  it('binary→无→binary：文件路径记忆还原', async () => {
    const draft = makeDraft({ id: 'ep-bin-mem' })
    draft.request.body = { mode: 'binary', path: '/tmp/a.bin' }
    const wrapper = mountPanel(reactive(draft))
    const tabBtn = (label: string) =>
      wrapper.findAll('.seg-item').find((b) => b.text() === label)!

    await tabBtn('无').trigger('click')
    await tabBtn('binary').trigger('click')
    expect(draft.request.body).toEqual({ mode: 'binary', path: '/tmp/a.bin' })
  })

  it('urlencoded↔form-data 直接切换：保持实时转换，不读旧记忆', async () => {
    const draft = makeDraft({ id: 'ep-conv-mem' })
    draft.request.body = {
      mode: 'urlencoded',
      fields: [{ key: 'a', value: '1', enabled: true, description: '' }],
    }
    const wrapper = mountPanel(reactive(draft))
    const tabBtn = (label: string) =>
      wrapper.findAll('.seg-item').find((b) => b.text() === label)!

    await tabBtn('form-data').trigger('click')
    expect(draft.request.body).toEqual({
      mode: 'multipart',
      fields: [{ key: 'a', value_type: 'text', value: '1', enabled: true }],
    })
  })

  it('raw(JSON)→无→raw：还原 JSON 子类型与文本，而非默认 text', async () => {
    const draft = jsonDraft('{"a":1}')
    draft.request.headers.push({ key: 'Content-Type', value: 'application/json', enabled: true, description: '' })
    const wrapper = mountPanel(draft)
    const tabBtn = (label: string) =>
      wrapper.findAll('.seg-item').find((b) => b.text() === label)!

    await tabBtn('无').trigger('click')
    expect(draft.request.body.mode).toBe('none')

    await tabBtn('raw').trigger('click')
    // 子类型 + 文本 + Content-Type 全部还原
    expect(draft.request.body).toEqual({ mode: 'json', raw: '{"a":1}' })
    expect(
      draft.request.headers.find((h) => h.key.toLowerCase() === 'content-type')?.value,
    ).toBe('application/json')
    // 编辑器回到 JSON 视图
    expect(wrapper.findComponent({ name: 'JsonEditor' }).exists()).toBe(true)
  })
})
