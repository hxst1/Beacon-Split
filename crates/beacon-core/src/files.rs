use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Largest file Beacon will open in the editor.
///
/// The editor is a convenience, not the point of the application; anything past
/// this is better opened somewhere built for it.
pub const MAX_EDITABLE_BYTES: u64 = 2 * 1024 * 1024;

/// How much of a file is inspected before deciding it is not text.
const SNIFF_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    /// Path relative to the project root, always with `/` separators.
    pub path: String,
    pub kind: EntryKind,
    pub hidden: bool,
}

/// A file as it was when Beacon read it.
///
/// The revision is what makes it safe to write back. Beacon exists to work
/// alongside Claude, and Claude edits files that are open — so "save what is in
/// the buffer" is a request to overwrite whatever happened in between, and
/// nobody means that.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRead {
    #[serde(flatten)]
    pub contents: FileContents,
    /// Changes whenever the file does. `None` when the filesystem would not say.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}

/// What came back from opening a file.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileContents {
    #[serde(rename_all = "camelCase")]
    Text { text: String },
    /// Not text, so the editor shows what it is rather than mangling it.
    #[serde(rename_all = "camelCase")]
    Binary { size: u64 },
    #[serde(rename_all = "camelCase")]
    TooLarge { size: u64 },
}

/// Resolves a project-relative path, refusing anything that escapes the root.
///
/// Every file operation goes through here. The rule is enforced on the resolved
/// path rather than on the text, because `a/../../b` and a symlink pointing
/// outside both look innocent as strings.
pub fn resolve_within(root: &Path, relative: &str) -> Result<PathBuf> {
    let candidate = Path::new(relative);
    if candidate.is_absolute() {
        return Err(CoreError::invalid("path must be relative to the project"));
    }
    if candidate
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(CoreError::invalid("path must stay inside the project"));
    }

    let root = root
        .canonicalize()
        .map_err(|err| CoreError::io(root, err))?;
    let joined = root.join(candidate);

    // The target may not exist yet — creating a file is a normal case — so the
    // deepest existing ancestor is what gets checked.
    let mut existing = joined.as_path();
    let resolved = loop {
        match existing.canonicalize() {
            Ok(path) => break path,
            Err(_) => match existing.parent() {
                Some(parent) => existing = parent,
                None => return Err(CoreError::invalid("path must stay inside the project")),
            },
        }
    };

    if !resolved.starts_with(&root) {
        return Err(CoreError::invalid("path must stay inside the project"));
    }
    Ok(joined)
}

/// Expresses a path relative to the root, with `/` separators.
fn relative_of(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// One directory's contents, directories first and then by name.
///
/// Not recursive: the tree expands a level at a time, so opening a project with
/// a large `node_modules` costs nothing until someone asks for it.
pub fn list_dir(root: &Path, relative: &str) -> Result<Vec<DirEntry>> {
    let dir = resolve_within(root, relative)?;
    let reader = std::fs::read_dir(&dir).map_err(|err| CoreError::io(&dir, err))?;
    let root = root
        .canonicalize()
        .map_err(|err| CoreError::io(root, err))?;

    let mut entries = Vec::new();
    for entry in reader {
        // A child we cannot stat — permission denied, or a file that vanished
        // while the directory was being read — costs us that one row. Failing
        // the listing instead leaves an expanded folder showing nothing, which
        // reads as an empty directory rather than as a problem.
        let Ok(entry) = entry else { continue };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        let name = entry.file_name().to_string_lossy().into_owned();
        let path = relative_of(&root, &entry.path());

        let kind = if file_type.is_symlink() {
            symlink_kind(&root, &path)
        } else if file_type.is_dir() {
            EntryKind::Directory
        } else {
            EntryKind::File
        };

        entries.push(DirEntry {
            hidden: name.starts_with('.'),
            path,
            name,
            kind,
        });
    }

    entries.sort_by(|a, b| {
        let folder_first = directory_rank(a).cmp(&directory_rank(b));
        folder_first.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

/// What a symlink behaves as, which is not always what it is.
///
/// A link to a directory — pnpm's `node_modules`, a package linked across a
/// monorepo — has to expand like the directory it points at. Called a symlink
/// it gets no twisty, and clicking it asks the editor to read a directory as a
/// file. Following it is only safe once `resolve_within` has agreed the target
/// is still inside the project, and `metadata` refuses a link that loops, so a
/// link out of the project and a link back onto itself both stay symlinks:
/// visible, and with nothing to open.
fn symlink_kind(root: &Path, relative: &str) -> EntryKind {
    let Ok(path) = resolve_within(root, relative) else {
        return EntryKind::Symlink;
    };
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.is_dir() => EntryKind::Directory,
        _ => EntryKind::Symlink,
    }
}

fn directory_rank(entry: &DirEntry) -> u8 {
    match entry.kind {
        EntryKind::Directory => 0,
        _ => 1,
    }
}

pub fn read_file(root: &Path, relative: &str) -> Result<FileRead> {
    let path = resolve_within(root, relative)?;
    let metadata = std::fs::metadata(&path).map_err(|err| CoreError::io(&path, err))?;
    let size = metadata.len();
    let revision = revision_of(&metadata);

    if size > MAX_EDITABLE_BYTES {
        return Ok(FileRead {
            contents: FileContents::TooLarge { size },
            revision,
        });
    }

    let bytes = std::fs::read(&path).map_err(|err| CoreError::io(&path, err))?;
    if looks_binary(&bytes) {
        return Ok(FileRead {
            contents: FileContents::Binary { size },
            revision,
        });
    }

    let contents = match String::from_utf8(bytes) {
        Ok(text) => FileContents::Text { text },
        // Valid bytes that are not UTF-8: treat as binary rather than lose them
        // to replacement characters on the way back out.
        Err(_) => FileContents::Binary { size },
    };
    Ok(FileRead { contents, revision })
}

/// A stamp that changes whenever the file does.
///
/// Modification time and size together: time alone can repeat within a
/// filesystem's granularity, and a same-length edit inside that window is
/// exactly the kind of change an editor must not miss.
fn revision_of(metadata: &std::fs::Metadata) -> Option<u64> {
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some(modified.as_nanos() as u64 ^ (metadata.len().rotate_left(17)))
}

/// What the file's revision is now, without reading it.
pub fn revision(root: &Path, relative: &str) -> Result<Option<u64>> {
    let path = resolve_within(root, relative)?;
    match std::fs::metadata(&path) {
        Ok(metadata) => Ok(revision_of(&metadata)),
        // A file that is gone has no revision, which is itself worth knowing.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(CoreError::io(&path, err)),
    }
}

/// How a write ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "outcome")]
pub enum WriteOutcome {
    /// The revision is the one the file now has, read straight after the
    /// write. The editor needs it to save again, and asking for it in a second
    /// call would leave a window in which someone else's write becomes the
    /// stamp we believe is ours.
    Written { revision: Option<u64> },
    /// The file changed since it was read. Nothing was written.
    Stale,
}

/// A NUL byte near the start is the usual signal, and the cheapest one.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(SNIFF_BYTES).any(|byte| *byte == 0)
}

/// Writes a file back, refusing if it changed since it was read.
///
/// `expected` is the revision the editor was working from. Passing `None` means
/// "write regardless", which is what an explicit overwrite asks for.
pub fn write_file(
    root: &Path,
    relative: &str,
    text: &str,
    expected: Option<u64>,
) -> Result<WriteOutcome> {
    let path = resolve_within(root, relative)?;

    if let Some(expected) = expected {
        let current = revision(root, relative)?;
        // A file that has since been deleted counts as changed: recreating it
        // silently is not what saving meant either.
        if current != Some(expected) {
            return Ok(WriteOutcome::Stale);
        }
    }

    write_atomically(&path, text)?;

    let revision = std::fs::metadata(&path)
        .ok()
        .and_then(|metadata| revision_of(&metadata));
    Ok(WriteOutcome::Written { revision })
}

/// Writes through a temporary file in the same directory and renames it over
/// the target.
///
/// `fs::write` truncates first and then fills, so a crash or a full disk
/// halfway through leaves the user with a shorter file and no copy of what was
/// there. A rename within one filesystem is atomic: the file is either the old
/// one or the new one, never a half-written one.
fn write_atomically(path: &Path, text: &str) -> Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();

    let mut temp = tempfile::Builder::new()
        .prefix(&format!(".{name}."))
        .suffix(".beacon")
        .tempfile_in(parent)
        .map_err(|err| CoreError::io(parent, err))?;

    // The rename gives the file the temporary's permissions, so an executable
    // script would come back not executable without this.
    if let Ok(existing) = std::fs::metadata(path) {
        let _ = temp.as_file().set_permissions(existing.permissions());
    }

    temp.write_all(text.as_bytes())
        .map_err(|err| CoreError::io(path, err))?;
    // Durability before visibility: the rename is atomic, but only for
    // contents that actually reached the disk.
    temp.as_file()
        .sync_all()
        .map_err(|err| CoreError::io(path, err))?;
    temp.persist(path)
        .map_err(|err| CoreError::io(path, err.error))?;
    Ok(())
}

pub fn create_file(root: &Path, relative: &str) -> Result<()> {
    let path = resolve_within(root, relative)?;
    if path.exists() {
        return Err(CoreError::invalid(format!(
            "{} already exists",
            path.file_name().unwrap_or_default().to_string_lossy()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| CoreError::io(parent, err))?;
    }
    std::fs::write(&path, "").map_err(|err| CoreError::io(&path, err))
}

pub fn create_dir(root: &Path, relative: &str) -> Result<()> {
    let path = resolve_within(root, relative)?;
    if path.exists() {
        return Err(CoreError::invalid(format!(
            "{} already exists",
            path.file_name().unwrap_or_default().to_string_lossy()
        )));
    }
    std::fs::create_dir_all(&path).map_err(|err| CoreError::io(&path, err))
}

/// Renames within the project. Both ends are checked, so a rename cannot be
/// used to move something out.
pub fn rename(root: &Path, from: &str, to: &str) -> Result<()> {
    let source = resolve_within(root, from)?;
    let target = resolve_within(root, to)?;
    // `README.md` to `readme.md` is a rename people make, and on a
    // case-insensitive volume the target "already exists" because it is the
    // file being renamed. Only a target that is a different entry is a clash.
    if target.exists() && !is_same_entry(&source, &target) {
        return Err(CoreError::invalid(format!(
            "{} already exists",
            target.file_name().unwrap_or_default().to_string_lossy()
        )));
    }
    std::fs::rename(&source, &target).map_err(|err| CoreError::io(&source, err))
}

/// Whether two paths name one entry on disk.
///
/// Compared by device and inode rather than by canonical path: macOS
/// `realpath` hands back the spelling it was given, so `README.md` and
/// `readme.md` canonicalise to two different strings while being one file —
/// exactly the case this has to recognise.
#[cfg(unix)]
fn is_same_entry(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn is_same_entry(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Copies a file or directory beside itself, as `name copy`, `name copy 2`, …
pub fn duplicate(root: &Path, relative: &str) -> Result<String> {
    let source = resolve_within(root, relative)?;
    let parent = source
        .parent()
        .ok_or_else(|| CoreError::invalid("cannot duplicate the project root"))?;
    let target = available_name(parent, &source);

    copy_path(&source, &target)?;
    let root = root
        .canonicalize()
        .map_err(|err| CoreError::io(root, err))?;
    Ok(relative_of(&root, &target))
}

/// Copies something into a directory, e.g. a paste.
pub fn copy_into(root: &Path, source_relative: &str, target_dir: &str) -> Result<String> {
    let source = resolve_within(root, source_relative)?;
    let directory = resolve_within(root, target_dir)?;

    let name = source
        .file_name()
        .ok_or_else(|| CoreError::invalid("nothing to copy"))?;
    let mut target = directory.join(name);
    if target.exists() {
        target = available_name(&directory, &source);
    }
    // Copying a directory into itself would recurse forever.
    if directory.starts_with(&source) {
        return Err(CoreError::invalid("cannot paste a folder into itself"));
    }

    copy_path(&source, &target)?;
    let root = root
        .canonicalize()
        .map_err(|err| CoreError::io(root, err))?;
    Ok(relative_of(&root, &target))
}

/// The first free `name copy`, `name copy 2`, … beside an existing entry.
fn available_name(parent: &Path, source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = source
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();

    for attempt in 1..1000 {
        let suffix = if attempt == 1 {
            " copy".to_string()
        } else {
            format!(" copy {attempt}")
        };
        let candidate = parent.join(format!("{stem}{suffix}{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem} copy{extension}"))
}

fn copy_path(source: &Path, target: &Path) -> Result<()> {
    let metadata = std::fs::metadata(source).map_err(|err| CoreError::io(source, err))?;
    if metadata.is_dir() {
        copy_dir(source, target)
    } else {
        std::fs::copy(source, target)
            .map(|_| ())
            .map_err(|err| CoreError::io(source, err))
    }
}

fn copy_dir(source: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target).map_err(|err| CoreError::io(target, err))?;
    for entry in std::fs::read_dir(source).map_err(|err| CoreError::io(source, err))? {
        let entry = entry.map_err(|err| CoreError::io(source, err))?;
        let child = target.join(entry.file_name());
        copy_path(&entry.path(), &child)?;
    }
    Ok(())
}

/// Moves an entry to the system trash.
///
/// Deliberately not a delete. This is the only destructive file operation
/// Beacon offers, and it should be one the user can undo from their own file
/// manager. See `docs/DECISIONS.md`, ADR-019.
pub fn move_to_trash(root: &Path, relative: &str) -> Result<()> {
    let path = resolve_within(root, relative)?;
    if relative.trim().is_empty() {
        return Err(CoreError::invalid("cannot delete the project root"));
    }
    trash::delete(&path).map_err(|err| CoreError::session("could not move to trash", err))
}

/// Directories never worth walking for a file list.
///
/// Only consulted when the project is not a repository — a repository's own
/// ignore rules are better than any list we could keep here.
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".venv",
    "venv",
    "__pycache__",
    "vendor",
    ".cache",
];

/// Stops a walk of a pathological directory tree from becoming the whole app.
pub const MAX_LISTED_FILES: usize = 50_000;

/// Every file in a project, for quick open.
///
/// A repository is listed with `git ls-files`, which respects the user's own
/// ignore rules and is far faster than walking. Anything else is walked with a
/// fixed skip list, which is a poor substitute but only applies where there is
/// nothing better to go on.
pub fn list_project_files(root: &Path) -> Result<Vec<String>> {
    if crate::git::is_repository(root) {
        if let Ok(listed) = crate::git::list_files(root) {
            return Ok(listed);
        }
        // A repository git refuses to read is still a folder we can walk.
    }

    let root = root
        .canonicalize()
        .map_err(|err| CoreError::io(root, err))?;
    let mut found = Vec::new();
    walk(&root, &root, &mut found);
    found.sort();
    Ok(found)
}

fn walk(root: &Path, dir: &Path, found: &mut Vec<String>) {
    if found.len() >= MAX_LISTED_FILES {
        return;
    }
    let Ok(reader) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in reader.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            if SKIPPED_DIRS.contains(&name.as_ref()) {
                continue;
            }
            // Not followed: a symlinked directory can point back up the tree.
            if file_type.is_symlink() {
                continue;
            }
            walk(root, &entry.path(), found);
        } else if file_type.is_file() {
            found.push(relative_of(root, &entry.path()));
            if found.len() >= MAX_LISTED_FILES {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join(".env"), "SECRET=1").unwrap();
        std::fs::write(dir.path().join("README.md"), "# hi").unwrap();
        dir
    }

    #[test]
    fn directories_come_first_then_names_case_insensitively() {
        let dir = project();
        let names: Vec<_> = list_dir(dir.path(), "")
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(names, vec!["src", ".env", "README.md"]);
    }

    #[test]
    fn dotfiles_are_marked_hidden_rather_than_omitted() {
        let dir = project();
        let entries = list_dir(dir.path(), "").unwrap();
        let env = entries.iter().find(|e| e.name == ".env").unwrap();
        assert!(env.hidden, "the tree decides what to show, not the backend");
    }

    #[test]
    fn paths_that_climb_out_of_the_project_are_refused() {
        let dir = project();
        for escape in ["../outside", "src/../../outside", "/etc/passwd"] {
            assert!(
                resolve_within(dir.path(), escape).is_err(),
                "accepted {escape:?}"
            );
        }
    }

    #[test]
    fn a_symlink_pointing_outside_the_project_is_refused() {
        let dir = project();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "nope").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), dir.path().join("link"))
            .unwrap();

        assert!(
            resolve_within(dir.path(), "link").is_err(),
            "a symlink is a path that looks innocent as a string"
        );
    }

    #[test]
    fn a_path_that_does_not_exist_yet_is_allowed_inside_the_project() {
        let dir = project();
        assert!(resolve_within(dir.path(), "src/new-file.rs").is_ok());
    }

    #[test]
    fn text_comes_back_as_text_and_binary_as_binary() {
        let dir = project();
        std::fs::write(dir.path().join("blob.bin"), [0u8, 1, 2, 3]).unwrap();

        assert!(matches!(
            read_file(dir.path(), "README.md").unwrap().contents,
            FileContents::Text { .. }
        ));
        assert!(matches!(
            read_file(dir.path(), "blob.bin").unwrap().contents,
            FileContents::Binary { .. }
        ));
    }

    #[test]
    fn a_write_is_refused_when_the_file_changed_underneath_it() {
        // The case this exists for: Claude edits a file while it is open, and
        // saving the buffer would throw that away without saying so.
        let dir = project();
        let read = read_file(dir.path(), "README.md").unwrap();

        std::fs::write(dir.path().join("README.md"), "# changed by something else").unwrap();

        let outcome = write_file(dir.path(), "README.md", "# my edit", read.revision).unwrap();
        assert_eq!(outcome, WriteOutcome::Stale);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("README.md")).unwrap(),
            "# changed by something else",
            "nothing should have been written"
        );
    }

    #[test]
    fn a_write_goes_through_when_nothing_moved() {
        let dir = project();
        let read = read_file(dir.path(), "README.md").unwrap();

        let outcome = write_file(dir.path(), "README.md", "# mine", read.revision).unwrap();
        assert!(matches!(outcome, WriteOutcome::Written { .. }));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("README.md")).unwrap(),
            "# mine"
        );
    }

    #[test]
    fn a_write_reports_the_revision_the_file_now_has() {
        // The editor saves again from this stamp. If it did not come back with
        // the write, the next save would be refused against one we made
        // obsolete ourselves.
        let dir = project();
        let read = read_file(dir.path(), "README.md").unwrap();

        let outcome = write_file(dir.path(), "README.md", "# mine", read.revision).unwrap();
        let WriteOutcome::Written { revision: reported } = outcome else {
            panic!("expected a write, got {outcome:?}");
        };
        assert_eq!(reported, revision(dir.path(), "README.md").unwrap());
        assert_ne!(
            reported, read.revision,
            "the file changed, so its stamp did"
        );
    }

    #[test]
    fn saving_twice_in_a_row_is_not_a_conflict() {
        let dir = project();
        let read = read_file(dir.path(), "README.md").unwrap();

        let WriteOutcome::Written { revision: first } =
            write_file(dir.path(), "README.md", "# one", read.revision).unwrap()
        else {
            panic!("the first write should go through");
        };
        assert!(matches!(
            write_file(dir.path(), "README.md", "# two", first).unwrap(),
            WriteOutcome::Written { .. }
        ));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("README.md")).unwrap(),
            "# two"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_saved_script_is_still_executable() {
        use std::os::unix::fs::PermissionsExt;

        // Writing through a temporary file and renaming means the file takes
        // the temporary's permissions unless they are carried over. Saving a
        // hook or a shell script must not disarm it.
        let dir = project();
        let script = dir.path().join("run.sh");
        std::fs::write(&script, "#!/bin/sh\necho hi\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        write_file(dir.path(), "run.sh", "#!/bin/sh\necho bye\n", None).unwrap();

        let mode = std::fs::metadata(&script).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755, "the executable bit was dropped");
    }

    #[test]
    fn a_write_leaves_no_temporary_behind() {
        let dir = project();
        write_file(dir.path(), "README.md", "# mine", None).unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".beacon"))
            .collect();
        assert!(leftovers.is_empty(), "left {leftovers:?} in the project");
    }

    #[test]
    fn passing_no_revision_overwrites_deliberately() {
        let dir = project();
        std::fs::write(dir.path().join("README.md"), "# theirs").unwrap();

        let outcome = write_file(dir.path(), "README.md", "# mine", None).unwrap();
        assert!(matches!(outcome, WriteOutcome::Written { .. }));
    }

    #[test]
    fn a_file_deleted_underneath_counts_as_changed() {
        let dir = project();
        let read = read_file(dir.path(), "README.md").unwrap();
        std::fs::remove_file(dir.path().join("README.md")).unwrap();

        // Recreating it silently is not what saving meant.
        assert_eq!(
            write_file(dir.path(), "README.md", "# mine", read.revision).unwrap(),
            WriteOutcome::Stale
        );
    }

    #[test]
    fn a_same_length_edit_still_changes_the_revision() {
        let dir = project();
        std::fs::write(dir.path().join("same.txt"), "aaaa").unwrap();
        let first = revision(dir.path(), "same.txt").unwrap();

        // Same size, and possibly the same second, which is why size alone and
        // time alone are both insufficient.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.path().join("same.txt"), "bbbb").unwrap();

        assert_ne!(revision(dir.path(), "same.txt").unwrap(), first);
    }

    #[test]
    fn duplicating_picks_the_next_free_name() {
        let dir = project();
        assert_eq!(
            duplicate(dir.path(), "README.md").unwrap(),
            "README copy.md"
        );
        assert_eq!(
            duplicate(dir.path(), "README.md").unwrap(),
            "README copy 2.md"
        );
    }

    #[test]
    fn duplicating_a_folder_copies_its_contents() {
        let dir = project();
        let copy = duplicate(dir.path(), "src").unwrap();
        assert_eq!(copy, "src copy");
        assert!(dir.path().join("src copy/main.rs").exists());
    }

    #[test]
    fn renaming_onto_an_existing_name_is_refused() {
        let dir = project();
        assert!(rename(dir.path(), "README.md", ".env").is_err());
    }

    #[test]
    fn a_folder_cannot_be_pasted_into_itself() {
        let dir = project();
        assert!(copy_into(dir.path(), "src", "src").is_err());
    }

    #[test]
    fn listing_a_plain_folder_skips_the_usual_noise() {
        let dir = project();
        std::fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
        std::fs::write(dir.path().join("node_modules/pkg/index.js"), "").unwrap();

        let listed = list_project_files(dir.path()).unwrap();
        assert!(listed.contains(&"src/main.rs".to_string()));
        assert!(listed.contains(&".env".to_string()));
        assert!(
            !listed.iter().any(|path| path.starts_with("node_modules")),
            "got: {listed:?}"
        );
    }

    #[test]
    fn creating_something_that_exists_is_refused() {
        let dir = project();
        assert!(create_file(dir.path(), "README.md").is_err());
        assert!(create_dir(dir.path(), "src").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_directory_in_the_project_is_listed_as_a_directory() {
        // pnpm's `node_modules` and a monorepo's linked packages are these.
        let dir = project();
        std::os::unix::fs::symlink(dir.path().join("src"), dir.path().join("linked")).unwrap();

        let entries = list_dir(dir.path(), "").unwrap();
        let link = entries.iter().find(|entry| entry.name == "linked").unwrap();
        assert_eq!(link.kind, EntryKind::Directory);

        let inside: Vec<_> = list_dir(dir.path(), "linked")
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(inside, vec!["main.rs"], "it has to expand like what it is");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_directory_outside_the_project_is_not_a_directory() {
        let dir = project();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(outside.path().join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("elsewhere"), dir.path().join("escape"))
            .unwrap();

        let entries = list_dir(dir.path(), "").unwrap();
        let link = entries.iter().find(|entry| entry.name == "escape").unwrap();
        assert_eq!(
            link.kind,
            EntryKind::Symlink,
            "offering to expand it would offer a way out of the project"
        );
        assert!(list_dir(dir.path(), "escape").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_that_loops_is_listed_without_being_followed() {
        let dir = project();
        std::os::unix::fs::symlink("loop", dir.path().join("loop")).unwrap();

        let entries = list_dir(dir.path(), "").unwrap();
        let link = entries.iter().find(|entry| entry.name == "loop").unwrap();
        assert_eq!(link.kind, EntryKind::Symlink);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_that_points_at_nothing_still_appears() {
        let dir = project();
        std::os::unix::fs::symlink(dir.path().join("gone"), dir.path().join("dangling")).unwrap();

        let entries = list_dir(dir.path(), "").unwrap();
        let link = entries
            .iter()
            .find(|entry| entry.name == "dangling")
            .unwrap();
        assert_eq!(
            link.kind,
            EntryKind::Symlink,
            "a broken link is something the user should be able to see and delete"
        );
    }

    #[test]
    fn renaming_a_file_to_a_different_case_is_allowed() {
        // On a case-insensitive volume the target "exists" because it is the
        // source, and this rename used to come back as "readme.md already
        // exists".
        let dir = project();
        rename(dir.path(), "README.md", "readme.md").unwrap();

        let names: Vec<_> = list_dir(dir.path(), "")
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert!(names.contains(&"readme.md".to_string()), "got: {names:?}");
        assert!(!names.contains(&"README.md".to_string()), "got: {names:?}");
    }
}
