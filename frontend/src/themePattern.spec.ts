/**
 * 主题写法回归测试：组件内禁止 `:global(html[data-theme=...])` 覆盖。
 *
 * 实证：该写法经 Vite 构建后丢失（产物中无对应规则），导致浅色主题静默失效
 * （侧边栏 / 右键菜单曾全黑）。主题差异一律用 style.css 的 CSS 变量表达。
 */
import { describe, expect, it } from 'vitest'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

function vueFiles(dir: string): string[] {
  const out: string[] = []
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) {
      if (name === 'node_modules' || name === 'dist') continue
      out.push(...vueFiles(p))
    } else if (name.endsWith('.vue')) {
      out.push(p)
    }
  }
  return out
}

describe('主题写法', () => {
  it('不存在 :global(html[data-theme]) 覆盖（构建会丢弃）', () => {
    const offenders: string[] = []
    for (const file of vueFiles(join(process.cwd(), 'src'))) {
      const text = readFileSync(file, 'utf8')
      if (text.includes(':global(html[')) offenders.push(file)
    }
    expect(offenders).toEqual([])
  })
})
