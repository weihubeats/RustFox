/**
 * PathVariablesPanel 单测：路径占位抽取（:id / {id} / {{id}}）与 KV 表编辑回写。
 * 抽取是显式按钮触发（自动回写会让刚打开的接口被 isDirty 误判为「有改动」）。
 */
import { beforeEach, describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick, reactive } from 'vue'
import PathVariablesPanel from './PathVariablesPanel.vue'
import KeyValueTable from './ui/KeyValueTable.vue'
import { useLocaleStore } from '../stores/locale'
import { makeDraft } from '../testUtils/draftFixture'
import type { Endpoint, KeyValue } from '../types/foxApi'

beforeEach(() => {
  setActivePinia(createPinia())
  // 文案断言锁定中文（jsdom 默认语言为英文，跟随系统会解析出英文）
  useLocaleStore().setMode('zh')
})

function mountPanel(draft: Endpoint) {
  return mount(PathVariablesPanel, { props: { draft }, attachTo: document.body })
}

describe('PathVariablesPanel', () => {
  it('从路径抽取 :id / {id} / {{id}} 三种占位（端口与查询串不误判）', async () => {
    const draft = reactive(
      makeDraft({ path: 'https://example.com:8080/users/:id/posts/{slug}/{{lang}}?x=1' }),
    )
    const wrapper = mountPanel(draft)
    expect(wrapper.text()).toContain('路径变量 (0)')

    await wrapper.find('.pv-extract').trigger('click')

    expect(draft.request.path_variables.map((r) => r.key)).toEqual(['id', 'slug', 'lang'])
    expect(wrapper.text()).toContain('路径变量 (3)')
    wrapper.unmount()
  })

  it('重复抽取保留已有取值与手写行，不产生重复 key', async () => {
    const draft = reactive(makeDraft({ path: '/users/:id/teams/:id' }))
    draft.request.path_variables = [
      { key: 'id', value: '42', enabled: true, description: '用户 ID' },
      { key: 'note', value: '手写行', enabled: true, description: '' },
    ]
    const wrapper = mountPanel(draft)

    await wrapper.find('.pv-extract').trigger('click')

    expect(draft.request.path_variables).toEqual([
      { key: 'id', value: '42', enabled: true, description: '用户 ID' },
      { key: 'note', value: '手写行', enabled: true, description: '' },
    ])
    wrapper.unmount()
  })

  it('路径无占位：不写入任何行', async () => {
    const draft = reactive(makeDraft({ path: '/users/list' }))
    const wrapper = mountPanel(draft)

    await wrapper.find('.pv-extract').trigger('click')

    expect(draft.request.path_variables).toEqual([])
    expect(wrapper.text()).toContain('路径变量 (0)')
    wrapper.unmount()
  })

  it('path_variables 缺省时，KV 表编辑也能建立字段', async () => {
    const draft = reactive(makeDraft())
    delete (draft.request as Partial<typeof draft.request>).path_variables
    const wrapper = mountPanel(draft)

    wrapper.findComponent(KeyValueTable).vm.$emit('update:modelValue', [
      { key: 'id', value: '7', enabled: true, description: '' },
    ] satisfies KeyValue[])
    await nextTick()

    expect(draft.request.path_variables?.map((r) => [r.key, r.value])).toEqual([['id', '7']])
    wrapper.unmount()
  })
})
