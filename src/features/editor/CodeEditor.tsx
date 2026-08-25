import { useEffect, useRef } from 'react'

import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { bracketMatching, indentOnInput } from '@codemirror/language'
import { gotoLine, highlightSelectionMatches, search, searchKeymap } from '@codemirror/search'
import { Compartment, EditorState } from '@codemirror/state'
import {
  EditorView,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from '@codemirror/view'
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete'

import { languageFor } from './languages'
import { beaconTheme } from './theme'
import styles from './CodeEditor.module.css'

interface CodeEditorProps {
  /** Identifies the buffer; changing it swaps the document. */
  path: string
  /**
   * The palette in force. CodeMirror compiles a theme into a stylesheet when
   * the view is created, so changing it means rebuilding the view.
   */
  theme: 'dark' | 'light'
  initialText: string
  onChange: (text: string) => void
  onSave: (text: string) => void
}

/**
 * A CodeMirror instance for one file.
 *
 * The editor is explicitly not the point of Beacon, so this is the smallest set
 * of extensions that makes editing pleasant: history, search and replace,
 * bracket handling, line numbers. No LSP, no completion beyond brackets.
 */
export function CodeEditor({
  path,
  theme,
  initialText,
  onChange,
  onSave,
}: CodeEditorProps): React.ReactElement {
  const hostRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  // Callbacks are read through a ref so changing them never rebuilds the view.
  const handlers = useRef({ onChange, onSave })
  handlers.current = { onChange, onSave }

  useEffect(() => {
    const host = hostRef.current
    if (!host) return

    const language = new Compartment()

    const view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: initialText,
        extensions: [
          lineNumbers(),
          highlightActiveLine(),
          highlightActiveLineGutter(),
          history(),
          drawSelection(),
          indentOnInput(),
          bracketMatching(),
          closeBrackets(),
          search({ top: true }),
          highlightSelectionMatches(),
          EditorState.allowMultipleSelections.of(true),
          language.of([]),
          beaconTheme(),
          keymap.of([
            // Save before the defaults, so Cmd/Ctrl+S is ours everywhere.
            // Go to line, on the binding every editor uses for it.
            { key: 'Mod-g', preventDefault: true, run: gotoLine },
            {
              key: 'Mod-s',
              preventDefault: true,
              run: (target) => {
                handlers.current.onSave(target.state.doc.toString())
                return true
              },
            },
            ...closeBracketsKeymap,
            ...searchKeymap,
            ...historyKeymap,
            ...defaultKeymap,
            indentWithTab,
          ]),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) handlers.current.onChange(update.state.doc.toString())
          }),
        ],
      }),
    })
    viewRef.current = view

    // The grammar arrives after the first paint; the document is already
    // readable, and highlighting appearing a moment later is not a flash.
    let cancelled = false
    void languageFor(path).then((extension) => {
      if (cancelled || !extension) return
      view.dispatch({ effects: language.reconfigure(extension) })
    })

    return () => {
      cancelled = true
      view.destroy()
      viewRef.current = null
    }
    // Rebuilding on `path` is deliberate: a different file is a different
    // document and a different history. `theme` is here because the stylesheet
    // is compiled once, not read live.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, theme])

  return <div className={styles['root']} ref={hostRef} />
}
