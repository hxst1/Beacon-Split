import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { AT_MOST_EVERY_MS, SETTLE_MS, liveTree } from './liveTree'

const PJ = 'pj_1'

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

const watching = (): boolean => true

describe('re-reading the tree when a session has done something', () => {
  it('waits for the burst to stop rather than re-reading on every report', () => {
    const refresh = vi.fn()
    const live = liveTree(PJ, refresh, watching)

    // A turn: tokens, a tool, more tokens, none of it far enough apart to
    // settle. Re-reading between two tool calls is work nobody would see.
    for (let i = 0; i < 5; i += 1) {
      live.report(PJ)
      vi.advanceTimersByTime(SETTLE_MS - 100)
    }
    expect(refresh).not.toHaveBeenCalled()

    vi.advanceTimersByTime(SETTLE_MS)
    expect(refresh).toHaveBeenCalledTimes(1)
  })

  it('re-reads a session that never goes quiet anyway', () => {
    const refresh = vi.fn()
    const live = liveTree(PJ, refresh, watching)

    // A build scrolling past, or a spinner. Something is definitely happening,
    // which is the one case where waiting for silence would wait forever.
    for (let i = 0; i < 100; i += 1) {
      live.report(PJ)
      vi.advanceTimersByTime(50)
    }

    expect(refresh).toHaveBeenCalled()
    // Capped rather than continuous: a report every 50ms must not become a
    // directory listing every 50ms.
    expect(refresh.mock.calls.length).toBeLessThanOrEqual(
      Math.ceil((100 * 50) / AT_MOST_EVERY_MS) + 1,
    )
  })

  it('ignores a project whose tree is not this one', () => {
    const refresh = vi.fn()
    const live = liveTree(PJ, refresh, watching)

    live.report('pj_other')
    vi.advanceTimersByTime(AT_MOST_EVERY_MS * 2)

    expect(refresh).not.toHaveBeenCalled()
  })

  it('does nothing while nobody is looking', () => {
    const refresh = vi.fn()
    // The focus this window gets back re-reads everything anyway, so working
    // for a window in the background is work twice over.
    const live = liveTree(PJ, refresh, () => false)

    live.report(PJ)
    vi.advanceTimersByTime(AT_MOST_EVERY_MS * 2)

    expect(refresh).not.toHaveBeenCalled()
  })

  it('drops what it had scheduled when the tree goes away', () => {
    const refresh = vi.fn()
    const live = liveTree(PJ, refresh, watching)

    live.report(PJ)
    live.cancel()
    vi.advanceTimersByTime(AT_MOST_EVERY_MS * 2)

    // A project switched away from must not re-read into a store that has
    // already moved on to another one.
    expect(refresh).not.toHaveBeenCalled()
  })

  it('starts a fresh wait after it has re-read', () => {
    const refresh = vi.fn()
    const live = liveTree(PJ, refresh, watching)

    live.report(PJ)
    vi.advanceTimersByTime(SETTLE_MS)
    expect(refresh).toHaveBeenCalledTimes(1)

    live.report(PJ)
    vi.advanceTimersByTime(SETTLE_MS)
    expect(refresh).toHaveBeenCalledTimes(2)
  })
})
