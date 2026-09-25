/**
 * KeyValueTable 单测：Value 列 {{变量}} 自动补全（可选 varCandidates，不传不启用）。
 * 弹层 VarSuggest teleport 到 body，断言走 document 查询。
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import KeyValueTable, { type KVRow } from './KeyValueTable.vue'
import { useLocaleStore } from '../../stores/locale'

let wrapper: VueWrapper | null = null

beforeEach(() => {
  setActivePinia(createPinia())
  // 文案断言锁定中文（jsdom 默认语言为英文，跟随系统会解析出英文）
  useLocaleStore().setMode('zh')
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = null
  document.querySelectorAll('.vs-pop').forEach((el) => el.remove())
})

function mountTable(modelValue: KVRow[] = [], varCandidates?: string[]) {
  const props: { modelValue: KVRow[]; varCandidates?: string[] } = { modelValue }
  if (varCandidates) props.varCandidates = varCandidates
  wrapper = mount(KeyValueTable, { props, attachTo: document.body })
  return wrapper
}

function valueInput(w: VueWrapper): HTMLInputElement {
  return w.findAll('input.kvt-value')[0]!.element as HTMLInputElement
}

/** jsdom 下 setValue 不保证光标，显式设值 + 光标 + 派发 input（v-model 与补全都读 el.value）。 */
function typeInto(input: HTMLInputElement, text: string): void {
  input.value = text
  input.setSelectionRange(text.length, text.length)
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

function press(input: HTMLInputElement, key: string): void {
  input.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }))
}

function suggestions(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>('.vs-pop .vs-item')]
}

describe('KeyValueTable 变量补全', () => {
  it('输入 {{ 展开候选，方向键切换高亮，Enter 插入 {{name}} 并回写 v-model', async () => {
    const w = mountTable([], ['base_url', 'token', 'uuid_x'])
    const input = valueInput(w)

    typeInto(input, '{{')
    await nextTick()
    expect(suggestions().map((el) => el.textContent)).toEqual([
      '{{base_url}}',
      '{{token}}',
      '{{uuid_x}}',
    ])
    expect(suggestions()[0]!.classList.contains('vs-item-hl')).toBe(true)

    press(input, 'ArrowDown')
    await nextTick()
    expect(suggestions()[1]!.classList.contains('vs-item-hl')).toBe(true)

    press(input, 'Enter')
    await nextTick()
    expect(input.value).toBe('{{token}}')
    // emitted() 返回「每次 emit 的参数数组」，故多一层 [0] 取到 rows
    const emitted = w.emitted('update:modelValue') as unknown as KVRow[][][] | undefined
    // 行对象与调用方数组共享引用：v-model 回写后即为新值（其后仍有一条幽灵行）
    expect(emitted?.at(-1)?.[0]?.[0]).toMatchObject({ key: '', value: '{{token}}' })
    expect(document.querySelector('.vs-pop')).toBeNull()
  })

  it('继续输入按前缀过滤，Esc 关闭弹层', async () => {
    const w = mountTable([], ['base_url', 'token'])
    const input = valueInput(w)

    typeInto(input, '{{to')
    await nextTick()
    expect(suggestions().map((el) => el.textContent)).toEqual(['{{token}}'])

    press(input, 'Escape')
    await nextTick()
    expect(document.querySelector('.vs-pop')).toBeNull()
    // Esc 不改值（留给上层「清空地址栏」等行为处理）
    expect(input.value).toBe('{{to')
  })

  it('未传 varCandidates：输入 {{ 不弹候选（保持既有调用方行为）', async () => {
    const w = mountTable([{ key: 'a', value: '', enabled: true, description: '' }])
    const input = w.findAll('input.kvt-value')[0]!.element as HTMLInputElement

    typeInto(input, '{{')
    await nextTick()
    expect(document.querySelector('.vs-pop')).toBeNull()
  })
})
