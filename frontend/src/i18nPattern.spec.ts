/**
 * i18n 写法回归测试：src 业务代码禁止用户可见的硬编码中文文案。
 *
 * 语言切换经 stores/locale.ts 的 t()/tFallback() 生效；漏接的硬编码文案
 * 在英文模式下会原样露出中文。允许的例外：
 * - 字典本体（src/i18n/）；
 * - 注释（代码注释、CSS 块注释、HTML 注释，非用户可见）；
 * - 存库数据值（测试用例分类 '正向/负向/…'、模块默认名 '默认'）；
 * - 语言自称（'简体中文' 在任何语言下都显示原文）；
 * - 正则里用于匹配用户输入的中文（environment.ts 环境名归类）。
 */
import { describe, expect, it } from 'vitest'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

const SRC = join(process.cwd(), 'src')
const CJK = /[\u4e00-\u9fff]/

/** 存库数据值：任何文件中出现都不算文案（展示层经 caseCategoryLabel 等映射翻译）。 */
const DATA_TOKENS = /'?(全部|正向|负向|边界值|安全性|其他)'?/g

/** 每文件的白名单行（正则），用于无法归入数据值语义的特例。 */
const FILE_ALLOWLIST: Record<string, RegExp[]> = {
  'src/utils/environment.ts': [CJK], // 正则匹配用户输入的环境名，非展示文案
  'src/stores/workspace.ts': [/module_name: '默认'/], // 存库数据默认值，有按名匹配逻辑
  'src/components/SettingsDialog.vue': [/'简体中文'/], // 语言自称
  'src/utils/clipboard.ts': [/console\.error/], // 开发者日志，非 UI 文案
}

function sourceFiles(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) {
      if (name === 'node_modules' || name === 'dist' || name === 'i18n' || name === 'testUtils') continue
      sourceFiles(p, out)
    } else if ((name.endsWith('.vue') || name.endsWith('.ts')) && !name.endsWith('.spec.ts')) {
      out.push(p)
    }
  }
  return out
}

/** 去注释后按行检查残留 CJK（注释里的中文是给开发者看的，不在约束范围）。 */
function visibleCjkLines(text: string): string[] {
  let stripped = text.replace(/\/\*[\s\S]*?\*\//g, '') // 块注释（含 CSS）
  stripped = stripped.replace(/<!--[\s\S]*?-->/g, '') // HTML 注释
  const offenders: string[] = []
  for (const raw of stripped.split('\n')) {
    const line = raw.replace(/(?<!:)\/\/.*$/, '') // 行注释（https:// 不受影响）
    const withoutData = line.replace(DATA_TOKENS, '')
    if (CJK.test(withoutData)) offenders.push(raw.trim())
  }
  return offenders
}

describe('i18n 写法', () => {
  it('用户可见文案不硬编码中文（一律走 t()/tFallback()）', () => {
    const offenders: string[] = []
    for (const file of sourceFiles(SRC)) {
      const rel = file.slice(SRC.length + 1)
      const allow = FILE_ALLOWLIST[`src/${rel}`] ?? []
      const lines = visibleCjkLines(readFileSync(file, 'utf8')).filter(
        (line) => !allow.some((re) => re.test(line)),
      )
      for (const line of lines) offenders.push(`${rel}: ${line}`)
    }
    expect(offenders).toEqual([])
  })
})
