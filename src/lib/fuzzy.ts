/**
 * Subsequence matching with a score, for the palette and quick open.
 *
 * Written here rather than pulled in: the whole behaviour is thirty lines, and
 * what makes a fuzzy matcher feel right is the weighting, which is exactly the
 * part a dependency would decide for us.
 */

export interface Match {
  score: number
  /** Indices in the candidate that matched, for highlighting. */
  positions: number[]
}

/** Bonus for a match at the start of a word — `sc` should find `src/Config`. */
const BOUNDARY_BONUS = 12
/** Bonus for continuing a run, so contiguous matches beat scattered ones. */
const RUN_BONUS = 8
/** Penalty per character skipped, so shorter paths win when both match. */
const SKIP_PENALTY = 1

/**
 * Scores `candidate` against `query`, or returns null when it does not match.
 *
 * Matching is case-insensitive unless the query has an uppercase letter, the
 * convention people already expect from search everywhere else.
 */
export function fuzzyMatch(candidate: string, query: string): Match | null {
  if (!query) return { score: 0, positions: [] }

  const sensitive = query !== query.toLowerCase()
  const haystack = sensitive ? candidate : candidate.toLowerCase()
  const needle = sensitive ? query : query.toLowerCase()

  const positions: number[] = []
  let score = 0
  let cursor = 0
  let run = 0

  for (const char of needle) {
    if (char === ' ') continue

    const found = haystack.indexOf(char, cursor)
    if (found === -1) return null

    const skipped = found - cursor
    score -= skipped * SKIP_PENALTY

    if (skipped === 0 && positions.length > 0) {
      run += 1
      score += RUN_BONUS * run
    } else {
      run = 0
    }

    if (isBoundary(candidate, found)) score += BOUNDARY_BONUS

    positions.push(found)
    cursor = found + 1
  }

  // Prefer the shorter of two equally good matches.
  score -= candidate.length * 0.1
  return { score, positions }
}

/** A separator before it, or a capital following a lowercase. */
function isBoundary(text: string, index: number): boolean {
  if (index === 0) return true
  const previous = text[index - 1] ?? ''
  if (/[/\-_. ]/.test(previous)) return true
  const current = text[index] ?? ''
  return previous === previous.toLowerCase() && current !== current.toLowerCase()
}

export interface Ranked<T> {
  item: T
  match: Match
}

/** Filters and orders a list by how well each entry matches. */
export function rank<T>(items: T[], query: string, textOf: (item: T) => string, limit = 200): Array<Ranked<T>> {
  const ranked: Array<Ranked<T>> = []

  for (const item of items) {
    const match = fuzzyMatch(textOf(item), query)
    if (match) ranked.push({ item, match })
  }

  ranked.sort((a, b) => b.match.score - a.match.score)
  return ranked.slice(0, limit)
}
