/**
 * highlight 单测：代码生成面板的多语言轻量高亮——
 * 关键字 / 字符串 / 注释 / flag / 宏着色，且所有文本先 HTML 转义。
 */
import { describe, expect, it } from 'vitest'
import { escapeHtml, highlightCode, highlightJSON } from './highlight'

describe('highlightCode：JavaScript', () => {
  it('关键字、字符串、数字分别着色', () => {
    const html = highlightCode('js', `const url = 'https://x';\nawait fetch(url, 1);`)
    expect(html).toContain('<span class="hl-k">const</span>')
    expect(html).toContain('<span class="hl-k">await</span>')
    expect(html).toContain(`<span class="hl-s">'https://x'</span>`)
    expect(html).toContain('<span class="hl-n">1</span>')
  })

  it('行注释整段着色，HTML 特殊字符先转义', () => {
    const html = highlightCode('js', '// a<b & c\nlet x = 1;')
    expect(html).toContain('<span class="hl-c">// a&lt;b &amp; c</span>')
  })
})

describe('highlightCode：cURL / Rust', () => {
  it('cURL：# 注释、-X/--data flag、$ 变量着色', () => {
    const html = highlightCode('curl', `# upload\ncurl -X POST --data '{"a":1}' $URL`)
    expect(html).toContain('<span class="hl-c"># upload</span>')
    expect(html).toContain('<span class="hl-v">-X</span>')
    expect(html).toContain('<span class="hl-v">--data</span>')
    expect(html).toContain('<span class="hl-v">$URL</span>')
    expect(html).toContain(`<span class="hl-s">'{&quot;a&quot;:1}'</span>`)
  })

  it('Rust：fn 关键字与 println! 宏；!= 不吞感叹号', () => {
    const html = highlightCode('rust', `fn main() {\nprintln!("hi");\nassert!(1 != 2);\n}`)
    expect(html).toContain('<span class="hl-k">fn</span>')
    expect(html).toContain('<span class="hl-v">println!</span>')
    expect(html).toContain('<span class="hl-v">assert!</span>')
    expect(html).not.toContain('!=</span>')
  })

  it('未知语言兜底为纯转义', () => {
    expect(highlightCode('unknown' as never, '<p>&')).toBe('&lt;p&gt;&amp;')
  })
})

describe('既有工具回归', () => {
  it('escapeHtml / highlightJSON 行为不变', () => {
    expect(escapeHtml('<a href="x">')).toBe('&lt;a href=&quot;x&quot;&gt;')
    expect(highlightJSON('{"a":1}')).toContain('hl-k')
  })
})
