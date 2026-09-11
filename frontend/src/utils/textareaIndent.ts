/**
 * textareaIndent：代码类 textarea 的 Tab 缩进行为（JsonEditor / Body 原生输入 /
 * GraphQL 调试视图），与 CodeMirror 的 indentWithTab 对齐。
 *
 * - Tab：无选区或单行选区时在光标处插入 2 空格；多行选区整块缩进；
 * - Shift+Tab：覆盖行整块反缩进（每行至多去 2 空格，不足去空为止）；
 * - 改完派发 input 事件，驱动上层 v-model / 防抖回写；焦点保持不动
 *   （浏览器默认 Tab 会直接跳出编辑框）。
 */
const INDENT = '  '

/** 处理 textarea 上的 Tab / Shift+Tab；非 Tab 键直接返回。 */
export function handleTextareaTab(ta: HTMLTextAreaElement, e: KeyboardEvent): void {
  if (e.key !== 'Tab') return
  e.preventDefault()
  const value = ta.value
  const s = ta.selectionStart ?? 0
  const en = ta.selectionEnd ?? 0
  if (!e.shiftKey && !value.slice(s, en).includes('\n')) {
    ta.value = value.slice(0, s) + INDENT + value.slice(en)
    ta.selectionStart = ta.selectionEnd = s + INDENT.length
  } else {
    // 覆盖行：选区末尾恰在行首（...\n|）时不含下一行。
    const end = en > s && value[en - 1] === '\n' ? en - 1 : en
    const lineStart = value.lastIndexOf('\n', s - 1) + 1
    const lineEndIdx = value.indexOf('\n', end)
    const stop = lineEndIdx === -1 ? value.length : lineEndIdx
    const lines = value.slice(lineStart, stop).split('\n')
    let removedBeforeStart = 0
    let removedTotal = 0
    const next = lines.map((ln, i) => {
      if (!e.shiftKey) return INDENT + ln
      const cut = ln.startsWith(INDENT) ? 2 : ln.startsWith(' ') || ln.startsWith('\t') ? 1 : 0
      if (i === 0) removedBeforeStart = cut
      removedTotal += cut
      return ln.slice(cut)
    })
    ta.value = value.slice(0, lineStart) + next.join('\n') + value.slice(stop)
    if (e.shiftKey) {
      ta.selectionStart = Math.max(lineStart, s - removedBeforeStart)
      ta.selectionEnd = Math.max(ta.selectionStart, en - removedTotal)
    } else {
      // 每行行首都 < 选区末：起止统一后移。
      ta.selectionStart = s + INDENT.length
      ta.selectionEnd = en + INDENT.length * lines.length
    }
  }
  ta.dispatchEvent(new Event('input', { bubbles: true }))
}
