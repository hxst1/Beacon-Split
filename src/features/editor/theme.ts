import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { EditorView } from '@codemirror/view'
import { tags } from '@lezer/highlight'
import type { Extension } from '@codemirror/state'

/**
 * Beacon's editor theme.
 *
 * Written by hand rather than pulled from a theme package so it uses the same
 * surfaces and hairlines as everything else: transparent background over the
 * panel's blur, and the workspace accent for the cursor and the active line.
 *
 * CodeMirror cannot read CSS custom properties for every value it needs, so the
 * few it does not are literals matching `tokens.css`.
 */
const base = EditorView.theme(
  {
    '&': {
      height: '100%',
      backgroundColor: 'transparent',
      color: 'rgb(255 255 255 / 0.88)',
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
      color: 'rgb(255 255 255 / 0.22)',
      border: 'none',
      paddingRight: '4px',
    },
    '.cm-lineNumbers .cm-gutterElement': { padding: '0 6px 0 12px' },
    '.cm-activeLineGutter': {
      backgroundColor: 'transparent',
      color: 'rgb(255 255 255 / 0.5)',
    },
    '.cm-activeLine': { backgroundColor: 'rgb(255 255 255 / 0.025)' },

    '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--accent)', borderLeftWidth: '2px' },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': {
      backgroundColor: 'rgb(255 255 255 / 0.14)',
    },
    '.cm-selectionMatch': { backgroundColor: 'var(--accent-wash)' },
    '.cm-matchingBracket, .cm-nonmatchingBracket': {
      backgroundColor: 'rgb(255 255 255 / 0.08)',
      outline: 'none',
    },

    // The search panel is chrome, so it gets the same treatment as a popover.
    '.cm-panels': {
      backgroundColor: 'rgb(20 20 24 / 0.86)',
      backdropFilter: 'blur(18px) saturate(150%)',
      color: 'rgb(255 255 255 / 0.8)',
      borderTop: '1px solid rgb(255 255 255 / 0.09)',
      fontSize: '11px',
    },
    '.cm-panels input, .cm-panels button': {
      backgroundColor: 'rgb(0 0 0 / 0.3)',
      color: 'inherit',
      border: '1px solid rgb(255 255 255 / 0.09)',
      borderRadius: '5px',
      padding: '2px 6px',
      font: 'inherit',
    },
    '.cm-searchMatch': { backgroundColor: 'var(--accent-wash)' },
    '.cm-searchMatch-selected': { backgroundColor: 'var(--accent-glow)' },

    '.cm-tooltip': {
      backgroundColor: 'rgb(20 20 24 / 0.92)',
      border: '1px solid rgb(255 255 255 / 0.09)',
      borderRadius: '8px',
      backdropFilter: 'blur(18px)',
    },
    '.cm-tooltip-autocomplete ul li[aria-selected]': {
      backgroundColor: 'var(--accent-wash)',
      color: 'rgb(255 255 255 / 0.94)',
    },
  },
  { dark: true },
)

/**
 * Muted and low-contrast on purpose: reading code is the job, and a rainbow
 * competes with the rest of the window.
 */
const highlighting = HighlightStyle.define([
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
])

export const beaconTheme: Extension = [base, syntaxHighlighting(highlighting)]
