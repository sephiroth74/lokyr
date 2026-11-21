//! The branch of a folder's git repository, read from the repository's own
//! files, as a plugin's status shows it, without running git.

use std::io::Read;
use std::path::{Path, PathBuf};

/// The most folders, up from a folder, that the search of its repository
/// looks in.
const DEEPEST: usize = 64;

/// The most bytes of a repository's file that the search reads: of a
/// `.git` file, of a `HEAD`.
const MOST_READ: u64 = 4096;

/// The most characters of a branch's name.
const LONGEST: usize = 256;

/// The characters of a detached head's short hash.
const SHORT_HASH: usize = 7;

/// The branch of the git repository that holds `folder`, as its `HEAD`
/// tells: the branch that `ref: refs/heads/` names, or the other reference,
/// or a detached head's short hash; none out of a repository, of a `HEAD`
/// that does not read, or of one of another form; none either of a folder
/// or a repository on a volume that may not answer, as another machine's,
/// whose looks could hold the caller.
pub(crate) fn branch(folder: &Path) -> Option<String> {
    if lokyr_pty::may_wait(folder) {
        return None;
    }
    let git = repository(folder)?;
    if lokyr_pty::may_wait(&git) {
        return None;
    }
    let head = read(&git.join("HEAD"))?;
    let head = head.trim();
    let name = if let Some(reference) = head.strip_prefix("ref:") {
        let reference = reference.trim();
        reference.strip_prefix("refs/heads/").unwrap_or(reference)
    } else if head.len() >= SHORT_HASH && head.chars().all(|c| c.is_ascii_hexdigit()) {
        &head[..SHORT_HASH]
    } else {
        return None;
    };
    let fine = !name.is_empty()
        && name.chars().count() <= LONGEST
        && !name
            .chars()
            .any(|c| c.is_control() || lokyr_plugin_sdk::text::is_hidden(c));
    fine.then(|| name.to_owned())
}

/// The git folder of the repository that holds `folder`: the `.git` folder
/// of the nearest folder up from it that has one, or the folder that a
/// `.git` file names, as a worktree's or a submodule's, `gitdir: <path>`,
/// of its folder where relative; none of no `.git` within [`DEEPEST`]
/// folders, or of the nearest of another form.
fn repository(folder: &Path) -> Option<PathBuf> {
    for at in folder.ancestors().take(DEEPEST) {
        let git = at.join(".git");
        let Ok(metadata) = std::fs::metadata(&git) else {
            continue;
        };
        if metadata.is_dir() {
            return Some(git);
        }
        if !metadata.is_file() {
            return None;
        }
        let text = read(&git)?;
        let named = text.trim().strip_prefix("gitdir:")?.trim();
        return (!named.is_empty()).then(|| at.join(named));
    }
    None
}

/// The text of the file at `path`, [`MOST_READ`] bytes at most; none of a
/// file that does not read, or not of UTF-8, or of no regular file, as a
/// FIFO, whose open would wait for a writer, or a link to one.
fn read(path: &Path) -> Option<String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    // Opened of no wait, as a FIFO's open waits for a writer; then read of
    // a regular file alone, as the open file tells, whatever the path held
    // at a look before.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    file.take(MOST_READ).read_to_string(&mut text).ok()?;
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of the test's own, made empty, whose `.git` file of no
    /// repository stops every search within it.
    fn root(test: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("lokyr-git-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".git"), "not a repository\n").unwrap();
        root
    }

    /// Makes `folder` a repository's, of a `HEAD` of `head`.
    fn repository(folder: &Path, head: &str) {
        std::fs::create_dir_all(folder.join(".git")).unwrap();
        std::fs::write(folder.join(".git/HEAD"), head).unwrap();
    }

    #[test]
    fn a_folder_s_branch_is_its_repository_s_head_up_from_it() {
        let root = root("branch");
        let project = root.join("project");
        repository(&project, "ref: refs/heads/feature/blocks\n");
        let deep = project.join("src/app");
        std::fs::create_dir_all(&deep).unwrap();
        assert_eq!(branch(&project).as_deref(), Some("feature/blocks"));
        assert_eq!(branch(&deep).as_deref(), Some("feature/blocks"));
        // A folder that does not exist, within the repository, takes it
        // too, as the search walks the path up.
        assert_eq!(
            branch(&project.join("gone")).as_deref(),
            Some("feature/blocks")
        );
        // A detached head's short hash, and another reference whole.
        repository(&project, "0123456789abcdef0123456789abcdef01234567\n");
        assert_eq!(branch(&deep).as_deref(), Some("0123456"));
        repository(&project, "ref: refs/remotes/origin/main\n");
        assert_eq!(branch(&deep).as_deref(), Some("refs/remotes/origin/main"));
        // A `HEAD` of another form, or of a hidden character, tells none.
        repository(&project, "garbage\n");
        assert_eq!(branch(&deep), None);
        repository(&project, "ref: refs/heads/a\u{202E}b\n");
        assert_eq!(branch(&deep), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_worktree_s_branch_is_of_the_folder_that_its_git_file_names() {
        let root = root("worktree");
        let main = root.join("main");
        repository(&main, "ref: refs/heads/main\n");
        let gitdir = main.join(".git/worktrees/side");
        std::fs::create_dir_all(&gitdir).unwrap();
        std::fs::write(gitdir.join("HEAD"), "ref: refs/heads/side\n").unwrap();
        // Of a relative path, and of an absolute one.
        let side = root.join("side");
        std::fs::create_dir_all(&side).unwrap();
        std::fs::write(side.join(".git"), "gitdir: ../main/.git/worktrees/side\n").unwrap();
        assert_eq!(branch(&side).as_deref(), Some("side"));
        std::fs::write(side.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();
        assert_eq!(branch(&side.join("src")).as_deref(), Some("side"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `branch(folder)` within 2 seconds, or the test fails rather than
    /// waits.
    fn branch_soon(folder: &Path) -> Option<String> {
        let folder = folder.to_owned();
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(branch(&folder));
        });
        receive
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("the branch's read waited")
    }

    /// Makes a FIFO at `path`.
    fn fifo(path: &Path) {
        let made = std::process::Command::new("mkfifo")
            .arg(path)
            .status()
            .unwrap();
        assert!(made.success());
    }

    #[test]
    fn a_fifo_in_place_of_a_repository_s_file_tells_no_branch_at_once() {
        let root = root("fifo");
        // A FIFO `HEAD`, as a tar archive keeps one.
        let head = root.join("head");
        std::fs::create_dir_all(head.join(".git")).unwrap();
        fifo(&head.join(".git/HEAD"));
        assert_eq!(branch_soon(&head), None);
        // A link to a FIFO.
        let linked = root.join("linked");
        std::fs::create_dir_all(linked.join(".git")).unwrap();
        std::os::unix::fs::symlink(head.join(".git/HEAD"), linked.join(".git/HEAD")).unwrap();
        assert_eq!(branch_soon(&linked), None);
        // A FIFO `.git`, in place of a worktree's file.
        let dot = root.join("dot");
        std::fs::create_dir_all(&dot).unwrap();
        fifo(&dot.join(".git"));
        assert_eq!(branch_soon(&dot), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn out_of_a_repository_no_branch() {
        let root = root("none");
        // The root's `.git` of no repository stops the search, of none.
        let plain = root.join("plain/folder");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(branch(&plain), None);
        // A repository whose `HEAD` is not there.
        let empty = root.join("empty");
        std::fs::create_dir_all(empty.join(".git")).unwrap();
        assert_eq!(branch(&empty), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
