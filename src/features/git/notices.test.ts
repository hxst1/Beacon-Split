import { describe, expect, it } from 'vitest'

import { noNotices, reduceNotices, type NoticeEvent, type Notices } from './notices'

const after = (...events: NoticeEvent[]): Notices =>
  events.reduce(reduceNotices, noNotices)

describe('git panel notices', () => {
  it('keeps a failed push on screen while the status keeps polling', () => {
    const notices = after(
      { type: 'actionFailed', text: 'failed to push some refs' },
      { type: 'pollSucceeded' },
      { type: 'pollSucceeded' },
    )

    expect(notices.action).toEqual({ tone: 'error', text: 'failed to push some refs' })
  })

  it('reports what a push or a pull said rather than discarding it', () => {
    expect(after({ type: 'actionSucceeded', text: 'Everything up-to-date' }).action).toEqual({
      tone: 'report',
      text: 'Everything up-to-date',
    })
  })

  it('says nothing at all when an action succeeds silently', () => {
    expect(after({ type: 'actionSucceeded' }).action).toBeNull()
    expect(after({ type: 'actionSucceeded', text: '  \n ' }).action).toBeNull()
  })

  it('drops the last answer as soon as the user asks for something else', () => {
    expect(
      after({ type: 'actionFailed', text: 'nothing to commit' }, { type: 'actionStarted' }).action,
    ).toBeNull()
  })

  it('shows a failing status read and a failing action at the same time', () => {
    const notices = after(
      { type: 'actionFailed', text: 'failed to push some refs' },
      { type: 'pollFailed', text: 'dubious ownership in repository' },
    )

    expect(notices.action?.text).toBe('failed to push some refs')
    expect(notices.poll).toBe('dubious ownership in repository')
  })

  it('clears the status complaint once the status can be read again', () => {
    expect(
      after({ type: 'pollFailed', text: 'dubious ownership' }, { type: 'pollSucceeded' }).poll,
    ).toBeNull()
  })

  it('leaves the state alone when nothing it holds has changed', () => {
    const settled = after({ type: 'actionFailed', text: 'boom' })
    expect(reduceNotices(settled, { type: 'pollSucceeded' })).toBe(settled)

    // A read failing the same way every two seconds is one piece of news.
    const failing = after({ type: 'pollFailed', text: 'dubious ownership' })
    expect(reduceNotices(failing, { type: 'pollFailed', text: 'dubious ownership' })).toBe(failing)
  })
})
