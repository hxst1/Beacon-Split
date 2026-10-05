import { fuzzyMatch } from '@/lib/fuzzy'

/** A file that matched a search, split the way it is drawn. */
export interface FileHit {
  path: string
  name: string
  /** The folder it is in, or `''` at the project root. */
  dir: string
  /** Matched characters in `name`, for highlighting. */
  positions: number[]
}

/** Whether any part of a path is a dotfile or sits inside one. */
export function isHiddenPath(path: string): boolean {
  return path.split('/').some((part) => part.startsWith('.'))
}

/**
 * How far a match in the name is put ahead of one that needs the folders.
 *
 * Larger than any score a path can earn on its own, so `store` lists every
 * file called something like store before `src/app/tree.ts`.
 */
const NAME_FIRST = 10_000

/**
 * The project's files that match `query`, best first.
 *
 * The name is tried first, because it is what people search by. A path is
 * still found through its folders — `files/tree` finds what a name alone
 * would not — but ranks below any file whose name matched. Dotfiles stay out
 * unless the tree is showing them, so a search never turns up what the tree
 * beside it is hiding.
 */
export function searchFiles(
  files: string[],
  query: string,
  { showHidden = true, limit = 200 }: { showHidden?: boolean; limit?: number } = {},
): FileHit[] {
  if (!query.trim()) return []
  const scored: Array<{ hit: FileHit; score: number }> = []

  for (const path of files) {
    if (!showHidden && isHiddenPath(path)) continue
    const cut = path.lastIndexOf('/')
    const shift = cut + 1
    const name = path.slice(shift)
    const dir = cut === -1 ? '' : path.slice(0, cut)

    const byName = fuzzyMatch(name, query)
    if (byName) {
      scored.push({ hit: { path, name, dir, positions: byName.positions }, score: byName.score + NAME_FIRST })
      continue
    }

    const byPath = fuzzyMatch(path, query)
    if (!byPath) continue
    // Only what fell inside the name is marked; the folder is context.
    const positions = byPath.positions.filter((at) => at >= shift).map((at) => at - shift)
    scored.push({ hit: { path, name, dir, positions }, score: byPath.score })
  }

  scored.sort((a, b) => b.score - a.score)
  return scored.slice(0, limit).map(({ hit }) => hit)
}
