import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { EditorView } from '@codemirror/view'
import { tags } from '@lezer/highlight'
import type { Extension } from '@codemirror/state'

/**
 * Beacon's editor theme.
 *
 * Written by hand rather than pulled from a theme package so it uses the same
 * surfaces and hairlines as everything else, and so it follows the palette:
 * CodeMirror builds a stylesheet once, so the theme is rebuilt when the palette
 * changes rather than reading variables it cannot re-evaluate.
 */
const light = (): boolean => document.documentElement.dataset['theme'] === 'light'

const editorTheme = (): Extension => EditorView.theme(
  {
    '&': {
      height: '100%',
      backgroundColor: 'transparent',
      color: light() ? 'rgb(20 20 26 / 0.92)' : 'rgb(255 255 255 / 0.88)',
      fontSize: '12.5px',
    },
    '.cm-content': {
      fontFamily: "'SF Mono', 'JetBrains Mono', Menlo, 'DejaVu Sans Mono', monospace",
      padding: '8px 0',
      caretColor: 'var(--accent)',
    },
    '.cm-scroller': { lineHeight: '1.55', overflow: 'auto' },
    '&.cm-focused': { outline: 'none' },

    '.cm-gutters': {
      backgroundColor: 'transparent',
      color: light() ? 'rgb(20 20 26 / 0.3)' : 'rgb(255 255 255 / 0.22)',
      border: 'none',
      paddingRight: '4px',
    },
    '.cm-lineNumbers .cm-gutterElement': { padding: '0 6px 0 12px' },
    '.cm-activeLineGutter': {
      backgroundColor: 'transparent',
      color: light() ? 'rgb(20 20 26 / 0.55)' : 'rgb(255 255 255 / 0.5)',
    },
    '.cm-activeLine': {
      backgroundColor: light() ? 'rgb(0 0 0 / 0.03)' : 'rgb(255 255 255 / 0.025)',
    },

    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--accent)', borderLeftWidth: '2px' },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': {
      backgroundColor: light() ? 'rgb(0 0 0 / 0.12)' : 'rgb(255 255 255 / 0.14)',
    },
    '.cm-selectionMatch': { backgroundColor: 'var(--accent-wash)' },
    '.cm-matchingBracket, .cm-nonmatchingBracket': {
      backgroundColor: light() ? 'rgb(0 0 0 / 0.07)' : 'rgb(255 255 255 / 0.08)',
      outline: 'none',
    },

    // The search panel is chrome, so it gets the same treatment as a popover.
    '.cm-panels': {
      backgroundColor: light() ? 'rgb(255 255 255 / 0.94)' : 'rgb(20 20 24 / 0.94)',
      color: light() ? 'rgb(20 20 26 / 0.8)' : 'rgb(255 255 255 / 0.8)',
      borderTop: light() ? '1px solid rgb(0 0 0 / 0.1)' : '1px solid rgb(255 255 255 / 0.09)',
      fontSize: '11px',
    },
    '.cm-panels input, .cm-panels button': {
      backgroundColor: light() ? 'rgb(0 0 0 / 0.05)' : 'rgb(0 0 0 / 0.3)',
      color: 'inherit',
      border: light() ? '1px solid rgb(0 0 0 / 0.12)' : '1px solid rgb(255 255 255 / 0.09)',
      borderRadius: '5px',
      padding: '2px 6px',
      font: 'inherit',
    },
    '.cm-searchMatch': { backgroundColor: 'var(--accent-wash)' },
    '.cm-searchMatch-selected': { backgroundColor: 'var(--accent-glow)' },

    '.cm-tooltip': {
      backgroundColor: light() ? '#ffffff' : '#1c1c23',
      border: light() ? '1px solid rgb(0 0 0 / 0.12)' : '1px solid rgb(255 255 255 / 0.09)',
      borderRadius: '8px',
    },
    '.cm-tooltip-autocomplete ul li[aria-selected]': {
      backgroundColor: 'var(--accent-wash)',
      color: light() ? 'rgb(20 20 26 / 0.94)' : 'rgb(255 255 255 / 0.94)',
    },
  },
  { dark: !light() },
)

/**
 * Muted and low-contrast on purpose: reading code is the job, and a rainbow
 * competes with the rest of the window.
 *
 * The light hues are the dark ones darkened rather than a different palette —
 * the same code should look like the same code in either theme.
 */
const highlighting = (): HighlightStyle => HighlightStyle.define(light() ? LIGHT_TAGS : DARK_TAGS)

const DARK_TAGS = [
  { tag: [tags.comment, tags.lineComment, tags.blockComment], color: 'rgb(255 255 255 / 0.3)', fontStyle: 'italic' },
  { tag: [tags.keyword, tags.modifier, tags.controlKeyword], color: '#a48cff' },
  { tag: [tags.string, tags.special(tags.string)], color: '#8fd18a' },
  { tag: [tags.number, tags.bool, tags.null], color: '#e0a76a' },
  { tag: [tags.function(tags.variableName), tags.function(tags.propertyName)], color: '#69b7ff' },
  { tag: [tags.typeName, tags.className, tags.namespace], color: '#5ec8c0' },
  { tag: [tags.propertyName, tags.attributeName], color: '#c3cbe0' },
  { tag: [tags.variableName, tags.definition(tags.variableName)], color: 'rgb(255 255 255 / 0.88)' },
  { tag: [tags.operator, tags.punctuation, tags.separator], color: 'rgb(255 255 255 / 0.45)' },
  { tag: [tags.heading, tags.strong], color: 'rgb(255 255 255 / 0.94)', fontWeight: '600' },
  { tag: tags.emphasis, fontStyle: 'italic' },
  { tag: [tags.link, tags.url], color: '#69b7ff', textDecoration: 'underline' },
  { tag: tags.invalid, color: '#ff6b6b' },
]

const LIGHT_TAGS = [
  { tag: [tags.comment, tags.lineComment, tags.blockComment], color: 'rgb(20 20 26 / 0.42)', fontStyle: 'italic' },
  { tag: [tags.keyword, tags.modifier, tags.controlKeyword], color: '#6b3fd4' },
  { tag: [tags.string, tags.special(tags.string)], color: '#2f7a35' },
  { tag: [tags.number, tags.bool, tags.null], color: '#9a5b12' },
  { tag: [tags.function(tags.variableName), tags.function(tags.propertyName)], color: '#1b64bd' },
  { tag: [tags.typeName, tags.className, tags.namespace], color: '#1a7a74' },
  { tag: [tags.propertyName, tags.attributeName], color: '#3d4b66' },
  { tag: [tags.variableName, tags.definition(tags.variableName)], color: 'rgb(20 20 26 / 0.92)' },
  { tag: [tags.operator, tags.punctuation, tags.separator], color: 'rgb(20 20 26 / 0.5)' },
  { tag: [tags.heading, tags.strong], color: 'rgb(20 20 26 / 0.94)', fontWeight: '600' },
  { tag: tags.emphasis, fontStyle: 'italic' },
  { tag: [tags.link, tags.url], color: '#1b64bd', textDecoration: 'underline' },
  { tag: tags.invalid, color: '#c02828' },
]

/**
 * Built on demand, not once.
 *
 * CodeMirror compiles a theme into a stylesheet when it is created, so a theme
 * captured at module load would keep the palette it was born in.
 */
export function beaconTheme(): Extension {
  return [editorTheme(), syntaxHighlighting(highlighting())]
}
