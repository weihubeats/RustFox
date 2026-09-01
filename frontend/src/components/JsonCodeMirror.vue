<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Compartment, EditorState } from '@codemirror/state'
import { EditorView, highlightActiveLine, highlightActiveLineGutter, highlightSpecialChars, keymap, lineNumbers, placeholder } from '@codemirror/view'
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { json, jsonParseLinter } from '@codemirror/lang-json'
import { linter } from '@codemirror/lint'
import { bracketMatching, defaultHighlightStyle, foldGutter, indentOnInput, syntaxHighlighting, HighlightStyle } from '@codemirror/language'
import { tags } from '@lezer/highlight'
import { THEME_EVENT } from '../stores/theme'

const props = withDefaults(
  defineProps<{
    modelValue: string
    readonly?: boolean
    placeholderText?: string
    autofocus?: boolean
  }>(),
  { readonly: false, placeholderText: '', autofocus: false },
)

const emit = defineEmits<{
  'update:modelValue': [value: string]
}>()

const host = ref<HTMLElement | null>(null)
let view: EditorView | null = null
const readOnlyCompartment = new Compartment()
const themeCompartment = new Compartment()

/** 当前主题：以 <html> 的 data-theme 为准，并监听 rustfox:theme 全局事件联动。 */
const theme = ref<'dark' | 'light'>(readThemeFromDom())
function readThemeFromDom(): 'dark' | 'light' {
  return document.documentElement.getAttribute('data-theme') === 'light' ? 'light' : 'dark'
}
function onThemeEvent(e: Event): void {
  const mode = (e as CustomEvent<{ mode: 'dark' | 'light' }>).detail?.mode
  if (mode === 'dark' || mode === 'light') theme.value = mode
}

const darkHighlight = HighlightStyle.define([
  { tag: [tags.propertyName], color: '#c084fc' },
  { tag: [tags.string], color: '#34d399' },
  { tag: [tags.number], color: '#38bdf8' },
  { tag: [tags.bool, tags.null], color: '#fbbf24' },
  { tag: [tags.punctuation, tags.bracket, tags.brace], color: '#94a3b8' },
  { tag: [tags.invalid], color: '#f87171' },
  { tag: [tags.lineComment], color: '#64748b' },
])

const lightHighlight = HighlightStyle.define([
  { tag: [tags.propertyName], color: '#7c3aed' },
  { tag: [tags.string], color: '#059669' },
  { tag: [tags.number], color: '#2563eb' },
  { tag: [tags.bool, tags.null], color: '#b45309' },
  { tag: [tags.punctuation, tags.bracket, tags.brace], color: '#6b7280' },
  { tag: [tags.invalid], color: '#dc2626' },
  { tag: [tags.lineComment], color: '#9ca3af' },
])

const darkTheme = EditorView.theme({
  '&': {
    height: '100%',
    fontSize: '12px',
    color: '#e2e8f0',
  },
  '.cm-scroller': {
    fontFamily: 'var(--font-mono)',
    lineHeight: '1.6',
  },
  '.cm-content': {
    padding: '8px 0',
    caretColor: '#a78bfa',
  },
  '.cm-line': {
    padding: '0 8px',
  },
  '.cm-gutters': {
    backgroundColor: 'transparent',
    borderRight: '1px solid rgba(148, 163, 184, 0.15)',
    color: '#64748b',
  },
  '.cm-foldGutter .cm-gutterElement': {
    cursor: 'pointer',
  },
  '&.cm-focused': {
    outline: 'none',
  },
  '.cm-cursor': {
    borderLeftColor: '#a78bfa',
  },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, ::selection':
    {
      backgroundColor: 'rgba(168, 85, 247, 0.25) !important',
    },
  '.cm-activeLine': {
    backgroundColor: 'rgba(148, 163, 184, 0.06)',
  },
  '.cm-activeLineGutter': {
    backgroundColor: 'rgba(148, 163, 184, 0.06)',
  },
  '.cm-tooltip': {
    backgroundColor: '#1e293b',
    border: '1px solid #334155',
    color: '#e2e8f0',
  },
  '.cm-tooltip-lint': {
    fontSize: '12px',
  },
  '.cm-foldPlaceholder': {
    backgroundColor: 'rgba(148, 163, 184, 0.15)',
    color: '#94a3b8',
    border: 'none',
  },
  '.cm-searchMatch': {
    backgroundColor: 'rgba(251, 191, 36, 0.25)',
  },
})

const lightTheme = EditorView.theme({
  '&': {
    height: '100%',
    fontSize: '12px',
    color: '#1f2329',
  },
  '.cm-scroller': {
    fontFamily: 'var(--font-mono)',
    lineHeight: '1.6',
  },
  '.cm-content': {
    padding: '8px 0',
    caretColor: '#7c3aed',
  },
  '.cm-line': {
    padding: '0 8px',
  },
  '.cm-gutters': {
    backgroundColor: 'transparent',
    borderRight: '1px solid rgba(107, 114, 128, 0.2)',
    color: '#9ca3af',
  },
  '.cm-foldGutter .cm-gutterElement': {
    cursor: 'pointer',
  },
  '&.cm-focused': {
    outline: 'none',
  },
  '.cm-cursor': {
    borderLeftColor: '#7c3aed',
  },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, ::selection':
    {
      backgroundColor: 'rgba(124, 58, 237, 0.2) !important',
    },
  '.cm-activeLine': {
    backgroundColor: 'rgba(107, 114, 128, 0.08)',
  },
  '.cm-activeLineGutter': {
    backgroundColor: 'rgba(107, 114, 128, 0.08)',
  },
  '.cm-tooltip': {
    backgroundColor: '#ffffff',
    border: '1px solid #d0d5dd',
    color: '#1f2329',
  },
  '.cm-tooltip-lint': {
    fontSize: '12px',
  },
  '.cm-foldPlaceholder': {
    backgroundColor: 'rgba(107, 114, 128, 0.12)',
    color: '#6b7280',
    border: 'none',
  },
  '.cm-searchMatch': {
    backgroundColor: 'rgba(217, 119, 6, 0.2)',
  },
})

/** 当前生效主题对应的扩展集（主题样式 + 高亮规则）。 */
function currentThemeExtension() {
  const dark = theme.value === 'dark'
  return [
    dark ? darkTheme : lightTheme,
    syntaxHighlighting(dark ? darkHighlight : lightHighlight),
  ]
}

onMounted(() => {
  window.addEventListener(THEME_EVENT, onThemeEvent)
  if (!host.value) return
  view = new EditorView({
    parent: host.value,
    doc: props.modelValue,
    extensions: [
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      foldGutter(),
      indentOnInput(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      bracketMatching(),
      closeBrackets(),
      highlightActiveLine(),
      keymap.of([...closeBracketsKeymap, ...defaultKeymap, ...historyKeymap, indentWithTab]),
      json(),
      themeCompartment.of(currentThemeExtension()),
      linter(jsonParseLinter()),
      readOnlyCompartment.of(EditorState.readOnly.of(props.readonly)),
      placeholder(props.placeholderText),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          emit('update:modelValue', update.state.doc.toString())
        }
      }),
    ],
  })
  if (props.autofocus) view.focus()
})

watch(
  theme,
  () => {
    if (!view) return
    view.dispatch({ effects: themeCompartment.reconfigure(currentThemeExtension()) })
  },
)

watch(
  () => props.modelValue,
  (val) => {
    if (!view) return
    const current = view.state.doc.toString()
    if (current !== val) {
      view.dispatch({ changes: { from: 0, to: current.length, insert: val } })
    }
  },
)

watch(
  () => props.readonly,
  (val) => {
    view?.dispatch({ effects: readOnlyCompartment.reconfigure(EditorState.readOnly.of(val)) })
  },
)

function requestMeasure(): void {
  view?.requestMeasure()
}

defineExpose({ requestMeasure, focus: () => view?.focus() })

onBeforeUnmount(() => {
  window.removeEventListener(THEME_EVENT, onThemeEvent)
  view?.destroy()
  view = null
})
</script>

<template>
  <div ref="host" class="cm-host"></div>
</template>

<style scoped>
.cm-host {
  height: 100%;
  min-height: 0;
  overflow: hidden;
}
.cm-host :deep(.cm-editor) {
  height: 100%;
}
.cm-host :deep(.cm-scroller) {
  overflow: auto;
}
</style>