import {
  BookOpen,
  Braces,
  Container,
  Database,
  File,
  FileArchive,
  FileCode,
  FileText,
  Film,
  Folder,
  FolderOpen,
  GitBranch,
  Image,
  Lock,
  Package,
  Palette,
  Scale,
  Settings2,
  Terminal,
} from 'lucide-react'

import { fileKind, type FileKind } from './fileKind'

/**
 * One shape per kind of thing, from Lucide (ISC).
 *
 * Far fewer shapes than the table has kinds, and deliberately: a reader is
 * learning a handful of silhouettes, not a hundred. Two endings that mean the
 * same sort of thing should look the same.
 */
const SHAPES: Record<FileKind, typeof File> = {
  folder: Folder,
  folderOpen: FolderOpen,
  file: File,
  code: FileCode,
  json: Braces,
  text: FileText,
  readme: BookOpen,
  license: Scale,
  image: Image,
  media: Film,
  archive: FileArchive,
  config: Settings2,
  lock: Lock,
  git: GitBranch,
  database: Database,
  shell: Terminal,
  styles: Palette,
  markup: FileCode,
  package: Package,
  container: Container,
}

/**
 * The icon for a row of the file tree.
 *
 * Drawn in the row's own colour rather than a colour of its own. The tree is a
 * quiet list somebody scans while doing something else, and a column of
 * coloured badges would compete with the names, which are the part being read.
 *
 * `aria-hidden`, because the name beside it already says what this is and a
 * screen reader announcing "image, icon.svg" is saying it twice.
 */
export function FileIcon({
  name,
  isDirectory,
  expanded = false,
  className,
}: {
  name: string
  isDirectory: boolean
  expanded?: boolean
  className?: string | undefined
}): React.ReactElement {
  const Shape = SHAPES[fileKind(name, isDirectory, expanded)]

  return (
    <Shape
      className={className}
      size={13}
      strokeWidth={1.75}
      aria-hidden
      focusable={false}
      {...{ 'data-icon': true }}
    />
  )
}
