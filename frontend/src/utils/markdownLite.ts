/**
 * markdownLite：更新详情弹窗的极简 Markdown → HTML 渲染。
 *
 * Release Notes 来自 updater 渠道（本仓库 Releases 正文），仍按不可信输入处理：
 * 先整体 HTML 转义再做结构转换，链接协议白名单 http/https，杜绝 XSS。
 *
 * 支持：#~#### 标题、围栏代码块、无序（- 或 +）与有序（1.）列表、> 引用、
 * --- 分割线、**粗体**、*斜体*、`行内代码`、[文字](链接)；
 * 空行分段，段内单换行渲染为 <br>。
 * 不支持嵌套列表 / 表格 / 图片（更新说明用不到，需要时再扩展）。
 */

/** HTML 实体转义（& 先转，避免二次转义）。 */
function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

/**
 * 行内转换：先转义，再按 行内代码 → 粗体 → 斜体 → 链接 顺序处理。
 * 行内代码先摘出为占位符，内部文本不再参与后续转换。
 */
function inline(text: string): string {
  const codes: string[] = []
  let out = escapeHtml(text).replace(/`([^`]+)`/g, (_m, code: string) => {
    codes.push(`<code>${code}</code>`)
    return `\u0000${codes.length - 1}\u0000`
  })
  out = out.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  out = out.replace(/(^|[^*])\*([^*\n]+)\*(?!\*)/g, '$1<em>$2</em>')
  // 链接：转义后协议仍可辨（javascript: 不匹配 https? 白名单）
  out = out.replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2" rel="noreferrer">$1</a>')
  return out.replace(/\u0000(\d+)\u0000/g, (_m, i: string) => codes[Number(i)] ?? '')
}

/** Markdown 源文 → 安全 HTML（空文返回空串）。 */
export function renderMarkdownLite(source: string): string {
  if (!source.trim()) return ''
  const lines = source.replace(/\r\n?/g, '\n').split('\n')
  const out: string[] = []
  /** 当前列表类型（ul / ol），null = 不在列表中。 */
  let list: 'ul' | 'ol' | null = null
  /** 当前段落缓冲行。 */
  let para: string[] = []
  /** 代码块缓冲行，null = 不在代码块中。 */
  let code: string[] | null = null

  const closeList = (): void => {
    if (list) out.push(`</${list}>`)
    list = null
  }
  const closePara = (): void => {
    if (para.length) out.push(`<p>${para.join('<br>')}</p>`)
    para = []
  }
  const closeAll = (): void => {
    closePara()
    closeList()
  }

  for (const raw of lines) {
    const line = raw.trim()
    if (code) {
      if (line.startsWith('```')) {
        out.push(`<pre><code>${code.join('\n')}</code></pre>`)
        code = null
      } else {
        code.push(escapeHtml(raw))
      }
      continue
    }
    if (line.startsWith('```')) {
      closeAll()
      code = []
      continue
    }
    if (!line) {
      closeAll()
      continue
    }
    const heading = /^(#{1,4})\s+(.*)$/.exec(line)
    if (heading) {
      closeAll()
      const level = Math.min(heading[1].length + 2, 6) // # → h3（h1/h2 归弹窗标题层级）
      out.push(`<h${level}>${inline(heading[2])}</h${level}>`)
      continue
    }
    if (/^(-{3,}|\*{3,}|_{3,})$/.test(line)) {
      closeAll()
      out.push('<hr>')
      continue
    }
    const quote = /^>\s?(.*)$/.exec(line)
    if (quote) {
      closeAll()
      out.push(`<blockquote>${inline(quote[1])}</blockquote>`)
      continue
    }
    const bullet = /^[-*+]\s+(.*)$/.exec(line)
    if (bullet) {
      closePara()
      if (list !== 'ul') {
        closeList()
        list = 'ul'
        out.push('<ul>')
      }
      out.push(`<li>${inline(bullet[1])}</li>`)
      continue
    }
    const ordered = /^\d+[.)]\s+(.*)$/.exec(line)
    if (ordered) {
      closePara()
      if (list !== 'ol') {
        closeList()
        list = 'ol'
        out.push('<ol>')
      }
      out.push(`<li>${inline(ordered[1])}</li>`)
      continue
    }
    closeList()
    para.push(inline(line))
  }
  // 未闭合的代码块按已闭合输出（防残缺源文丢内容）
  if (code) out.push(`<pre><code>${code.join('\n')}</code></pre>`)
  closeAll()
  return out.join('')
}
