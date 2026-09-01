export interface RequestSequence {
  begin: () => number
  invalidate: () => void
  isCurrent: (request: number) => boolean
}

/** Last-started-wins sequencing for async status reads. */
export function createRequestSequence(): RequestSequence {
  let generation = 0

  return {
    begin: () => ++generation,
    invalidate: () => {
      generation += 1
    },
    isCurrent: (request) => request === generation,
  }
}
