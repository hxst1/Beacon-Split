import { describe, expect, it } from 'vitest'

import { createTurnClock, spellDuration } from './notify'

/** A clock you drive by hand, so a thirty-second turn takes no time to test. */
function stopwatch() {
  let now = 0
  return {
    tick: (ms: number) => {
      now += ms
    },
    read: () => now,
  }
}

describe('createTurnClock', () => {
  it('says nothing about a turn short enough that you were watching it', () => {
    const watch = stopwatch()
    const clock = createTurnClock(watch.read)

    clock.saw('a', 'working')
    watch.tick(4_000)

    expect(clock.saw('a', 'done')).toBeNull()
  })

  it('reports how long a turn ran once it is worth interrupting for', () => {
    const watch = stopwatch()
    const clock = createTurnClock(watch.read)

    clock.saw('a', 'working')
    watch.tick(90_000)

    expect(clock.saw('a', 'done')).toBe(90_000)
  })

  it('times a turn from its first tool, not its latest', () => {
    const watch = stopwatch()
    const clock = createTurnClock(watch.read)

    clock.saw('a', 'working')
    watch.tick(50_000)
    clock.saw('a', 'working')
    watch.tick(10_000)

    expect(clock.saw('a', 'done')).toBe(60_000)
  })

  it('keeps counting through a permission prompt', () => {
    const watch = stopwatch()
    const clock = createTurnClock(watch.read)

    clock.saw('a', 'working')
    watch.tick(20_000)
    clock.saw('a', 'waiting')
    watch.tick(20_000)

    expect(clock.saw('a', 'done')).toBe(40_000)
  })

  it('claims nothing about a session that finished without ever working', () => {
    const clock = createTurnClock(stopwatch().read)

    clock.saw('a', 'idle')

    expect(clock.saw('a', 'done')).toBeNull()
  })

  it('starts the next turn from scratch', () => {
    const watch = stopwatch()
    const clock = createTurnClock(watch.read)

    clock.saw('a', 'working')
    watch.tick(90_000)
    clock.saw('a', 'done')

    watch.tick(600_000)
    clock.saw('a', 'working')
    watch.tick(1_000)

    expect(clock.saw('a', 'done')).toBeNull()
  })

  it('times each project separately', () => {
    const watch = stopwatch()
    const clock = createTurnClock(watch.read)

    clock.saw('a', 'working')
    watch.tick(60_000)
    clock.saw('b', 'working')
    watch.tick(5_000)

    expect(clock.saw('b', 'done')).toBeNull()
    expect(clock.saw('a', 'done')).toBe(65_000)
  })

  it('forgets a project whose session ended', () => {
    const watch = stopwatch()
    const clock = createTurnClock(watch.read)

    clock.saw('a', 'working')
    watch.tick(90_000)
    clock.forget('a')

    expect(clock.saw('a', 'done')).toBeNull()
  })
})

describe('spellDuration', () => {
  it('reads seconds under a minute', () => {
    expect(spellDuration(45_000)).toBe('45s')
  })

  it('drops a zero remainder', () => {
    expect(spellDuration(120_000)).toBe('2m')
    expect(spellDuration(7_200_000)).toBe('2h')
  })

  it('keeps a remainder that carries information', () => {
    expect(spellDuration(252_000)).toBe('4m 12s')
    expect(spellDuration(5_400_000)).toBe('1h 30m')
  })
})
