//! The volumes whose file systems may not answer, as a network's, or one
//! that the system mounts as it is reached: of the system's table of mounts,
//! read without waiting for any of them, so that a look at whether a path
//! is there can leave such a volume alone rather than hold its thread.

use std::path::{Path, PathBuf};

/// A mount: where it is, and whether its file system may not answer.
type Mount = (PathBuf, bool);

/// The kinds of file systems of another machine, or that the system mounts
/// as they are reached, as their table names them.
const MAY_WAIT: [&str; 17] = [
    "nfs",
    "nfs4",
    "smbfs",
    "smb3",
    "cifs",
    "afpfs",
    "webdav",
    "autofs",
    "sshfs",
    "fuse.sshfs",
    "9p",
    "afs",
    "ceph",
    "glusterfs",
    "davfs",
    "macfuse",
    "osxfuse",
];

/// Whether `path`, absolute, is on a volume whose file system may not
/// answer, of the system's mounts as it tells them without waiting for any:
/// its mount, the deepest that holds it, of another machine or mounted as
/// it is reached; false where the table cannot be read.
pub fn may_wait(path: &Path) -> bool {
    may_wait_in(path, &mounts())
}

/// Whether `path` is on a mount of `mounts` that may not answer: the
/// deepest that holds it.
fn may_wait_in(path: &Path, mounts: &[Mount]) -> bool {
    mounts
        .iter()
        .filter(|(at, _)| path.starts_with(at))
        .max_by_key(|(at, _)| at.components().count())
        .is_some_and(|&(_, waits)| waits)
}

/// The system's mounts, each of where it is and whether it may not answer,
/// of `getfsstat`'s table as it holds it, of `MNT_NOWAIT`, asking none of
/// them.
#[cfg(target_os = "macos")]
fn mounts() -> Vec<Mount> {
    // SAFETY: of no buffer, `getfsstat` tells how many mounts there are,
    // and reads no memory of ours.
    let count = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    let Ok(count) = usize::try_from(count) else {
        return Vec::new();
    };
    // Room for a mount or two that came meanwhile.
    let room = count + 4;
    let mut stats: Vec<libc::statfs> = Vec::with_capacity(room);
    let Ok(size) = libc::c_int::try_from(room * std::mem::size_of::<libc::statfs>()) else {
        return Vec::new();
    };
    // SAFETY: the buffer holds `room` entries, `size` bytes, which
    // `getfsstat` fills at most, of `MNT_NOWAIT`, which waits for no file
    // system; it tells how many it filled.
    let filled = unsafe { libc::getfsstat(stats.as_mut_ptr(), size, libc::MNT_NOWAIT) };
    let Ok(filled) = usize::try_from(filled) else {
        return Vec::new();
    };
    // SAFETY: `getfsstat` filled the first `filled` entries, within the
    // buffer's room.
    unsafe { stats.set_len(filled.min(room)) };
    let text = |chars: &[libc::c_char]| {
        let bytes: Vec<u8> = chars
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c.cast_unsigned())
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    };
    stats
        .iter()
        .map(|stat| {
            let kind = text(&stat.f_fstypename);
            let local = stat.f_flags & libc::MNT_LOCAL as u32 != 0;
            let waits = !local || MAY_WAIT.contains(&kind.as_str());
            (PathBuf::from(text(&stat.f_mntonname)), waits)
        })
        .collect()
}

/// The system's mounts, each of where it is and whether it may not answer,
/// of `/proc/self/mounts`, which the kernel tells of its table, asking none
/// of them.
#[cfg(target_os = "linux")]
fn mounts() -> Vec<Mount> {
    let Ok(text) = std::fs::read_to_string("/proc/self/mounts") else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let (_, at, kind) = (fields.next()?, fields.next()?, fields.next()?);
            let waits = MAY_WAIT.contains(&kind) || kind.starts_with("fuse.");
            Some((PathBuf::from(unescaped(at)), waits))
        })
        .collect()
}

/// No mount's table of the other systems: no volume known to wait.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn mounts() -> Vec<Mount> {
    Vec::new()
}

/// A mount point as `/proc/self/mounts` writes it, its space, tab, newline
/// and backslash of octal escapes, as `\040`, decoded.
#[cfg(any(target_os = "linux", test))]
fn unescaped(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'\\')
            .then(|| text.get(at + 1..at + 4))
            .flatten()
            .and_then(|octal| u8::from_str_radix(octal, 8).ok());
        match escaped {
            Some(byte) => {
                out.push(byte);
                at += 4;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_waits_of_the_deepest_mount_that_holds_it() {
        let mounts = [
            (PathBuf::from("/"), false),
            (PathBuf::from("/Volumes/share"), true),
            (PathBuf::from("/Volumes/share/local"), false),
            (PathBuf::from("/net"), true),
        ];
        let waits = |path: &str| may_wait_in(Path::new(path), &mounts);
        assert!(!waits("/Users/me/notes.txt"));
        assert!(waits("/Volumes/share/notes.txt"));
        assert!(waits("/Volumes/share"));
        assert!(!waits("/Volumes/share/local/notes.txt"));
        assert!(waits("/net/host/x"));
        // A mount's name as a part's prefix alone holds nothing.
        assert!(!waits("/Volumes/shared/notes.txt"));
        assert!(!may_wait_in(Path::new("/x"), &[]));
    }

    #[test]
    fn the_system_s_mounts_read_without_waiting_hold_its_root_as_local() {
        let mounts = mounts();
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        assert!(
            mounts
                .iter()
                .any(|(at, waits)| at == Path::new("/") && !waits),
            "{mounts:?}"
        );
        assert!(!may_wait(Path::new("/")));
    }

    #[test]
    fn a_mount_point_s_escapes_are_decoded() {
        assert_eq!(unescaped("/media/my\\040disk"), "/media/my disk");
        assert_eq!(unescaped("/plain"), "/plain");
        assert_eq!(unescaped("/odd\\04"), "/odd\\04");
    }
}
