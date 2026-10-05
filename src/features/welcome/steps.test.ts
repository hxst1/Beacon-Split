import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

import { PANEL_LABELS } from '@/lib/layout'
import { anchorSelector, guideSteps, PANEL_STEPS, placeCard, type GuideFacts } from './steps'

const HINTS: Record<string, string> = {
  'palette.open': '⌘K',
  'quickOpen.open': '⌘P',
  'settings.open': '⌘,',
  'panel.toggle.codex': '⇧⌘O',
}

const facts = (overrides: Partial<GuideFacts> = {}): GuideFacts => ({
  hasProject: true,
  hidden: [],
  hint: (action) => HINTS[action],
  claudeInstalled: true,
  ...overrides,
})

const ids = (f: GuideFacts): string[] => guideSteps(f).map((step) => step.id)

describe('the welcome guide', () => {
  it('introduces every panel there is', () => {
    // A panel added to Beacon has to be added here as well, or this fails.
    expect(Object.keys(PANEL_STEPS).sort()).toEqual(Object.keys(PANEL_LABELS).sort())
  })

  it('asks for a first project, and points at no panel, until there is one', () => {
    const steps = ids(facts({ hasProject: false }))
    expect(steps).toContain('project')
    expect(steps.some((id) => id.startsWith('panel.'))).toBe(false)
  })

  it('walks round the panels once there is a project', () => {
    const steps = ids(facts())
    expect(steps).not.toContain('project')
    expect(steps.filter((id) => id.startsWith('panel.'))).toEqual([
      'panel.claude',
      'panel.codex',
      'panel.files',
      'panel.editor',
      'panel.terminal',
      'panel.git',
    ])
  })

  it('starts with the basics and ends at signing in, in the Claude panel', () => {
    const steps = guideSteps(facts())
    expect(steps[1]?.action).toBe('basics')
    const last = steps.at(-1)
    expect(last?.id).toBe('signIn')
    expect(last?.anchor).toEqual({ panel: 'claude' })
  })

  it('offers to show a hidden panel, with its shortcut when it has one', () => {
    const steps = guideSteps(facts({ hidden: ['codex', 'git'] }))
    const codex = steps.find((step) => step.id === 'panel.codex')
    const git = steps.find((step) => step.id === 'panel.git')
    const files = steps.find((step) => step.id === 'panel.files')

    expect(codex?.action).toBe('showPanel')
    expect(codex?.paragraphs.at(-1)).toContain('⇧⌘O')
    expect(git?.action).toBe('showPanel')
    expect(git?.paragraphs.at(-1)).toBe('It is hidden right now. Show it from here.')
    expect(files?.action).toBeUndefined()
  })

  it('names the shortcuts the user actually has', () => {
    const keyboard = guideSteps(facts()).find((step) => step.id === 'keyboard')
    expect(keyboard?.paragraphs.join(' ')).toContain('⌘K')
    expect(keyboard?.paragraphs.join(' ')).toContain('⌘,')

    // Unbound, it still says where things are, without a shortcut to press.
    const unbound = guideSteps(facts({ hint: () => undefined })).find((s) => s.id === 'keyboard')
    expect(unbound?.paragraphs.join(' ')).toContain('command palette')
    expect(unbound?.paragraphs.join(' ')).not.toContain('undefined')
  })

  it('says how to get Claude Code when it is not installed', () => {
    const signIn = guideSteps(facts({ claudeInstalled: false })).at(-1)
    expect(signIn?.paragraphs[0]).toContain('not installed')
  })

  it('points at hooks that are really there', () => {
    const source = (path: string): string =>
      readFileSync(fileURLToPath(new URL(path, import.meta.url)), 'utf8')

    expect(source('../../app/TitleBar.tsx')).toContain('data-region="titlebar"')
    expect(source('../../app/StatusBar.tsx')).toContain('data-region="statusbar"')
    expect(source('../../app/panels/Panel.tsx')).toContain('data-panel={id}')
    expect(anchorSelector({ panel: 'git' })).toBe('[data-panel="git"]')
    expect(anchorSelector({ region: 'titlebar' })).toBe('[data-region="titlebar"]')
  })
})

describe('placing the card', () => {
  const viewport = { width: 1200, height: 800 }
  const card = { width: 300, height: 200 }

  it('centres it when there is nothing to point at', () => {
    expect(placeCard(null, card, viewport)).toEqual({ top: 300, left: 450 })
  })

  it('puts it beside the target, right first', () => {
    const narrow = { top: 100, left: 40, width: 200, height: 500 }
    expect(placeCard(narrow, card, viewport)).toEqual({ top: 100, left: 254 })
  })

  it('goes left when the right has no room', () => {
    const atTheRight = { top: 100, left: 900, width: 280, height: 500 }
    expect(placeCard(atTheRight, card, viewport)).toEqual({ top: 100, left: 586 })
  })

  it('goes below a bar across the top', () => {
    const titlebar = { top: 0, left: 0, width: 1200, height: 42 }
    expect(placeCard(titlebar, card, viewport)).toEqual({ top: 56, left: 16 })
  })

  it('goes above a bar across the bottom', () => {
    const statusbar = { top: 776, left: 0, width: 1200, height: 24 }
    expect(placeCard(statusbar, card, viewport)).toEqual({ top: 562, left: 16 })
  })

  it('sits inside a target that fills the window, in its lower corner', () => {
    const everything = { top: 42, left: 0, width: 1200, height: 734 }
    expect(placeCard(everything, card, viewport)).toEqual({ top: 560, left: 884 })
  })

  it('never leaves the window', () => {
    const tiny = { width: 400, height: 300 }
    const big = { width: 380, height: 280 }
    for (const target of [null, { top: 0, left: 0, width: 400, height: 300 }]) {
      const { top, left } = placeCard(target, big, tiny)
      expect(top).toBeGreaterThanOrEqual(0)
      expect(left).toBeGreaterThanOrEqual(0)
    }
  })
})
