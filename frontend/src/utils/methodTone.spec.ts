/**
 * methodTone 单测：徽章/文本语义类映射与未知方法兜底。
 */
import { describe, expect, it } from 'vitest'
import { METHOD_TEXT_TONE, METHOD_TONE, methodTextTone, methodTone } from './methodTone'

describe('methodTone', () => {
  it('常用方法映射到对应语义工具类', () => {
    expect(methodTone('GET')).toContain('text-method-get')
    expect(methodTone('post')).toContain('bg-method-post/10')
    expect(methodTone('Put')).toContain('border-method-put/20')
    expect(methodTone('DELETE')).toContain('text-method-delete')
    expect(methodTone('PATCH')).toContain('text-method-patch')
  })

  it('未知方法兜底中性灰', () => {
    expect(methodTone('BREW')).toBe(METHOD_TONE.options)
    expect(methodTextTone('BREW')).toBe(METHOD_TEXT_TONE.options)
  })

  it('纯文本式只含文字色', () => {
    expect(methodTextTone('GET')).toBe('text-method-get')
    expect(methodTextTone('HEAD')).toBe('text-method-get')
    expect(methodTextTone('GRAPHQL')).toBe('text-method-patch')
  })

  it('映射表键覆盖全部徽章场景', () => {
    for (const m of ['get', 'post', 'put', 'delete', 'patch', 'graphql', 'head', 'options']) {
      expect(METHOD_TONE[m]).toBeTruthy()
    }
  })
})
