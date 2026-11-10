//! The workspace: the one directory the server reads and writes.
//!
//! Entry: [`Workspace`]. Every path a tool takes is relative to the root
//! (or absolute inside it). A path is resolved by canonicalizing its
//! longest existing prefix (which follows symbolic links) and appending
//! the rest; the result must lie under the canonical root, and the rest
//! may hold no `..`. A dangling symbolic link is refused, so a write
//! never goes through one. Files read are capped at `max_bytes`.
//!
//! The check runs before every read and write, and again on the opened
//! file: a read opens the path and then confirms that the opened file is
//! the one at its real path, under the root (on Unix by device and inode;
//! elsewhere by resolving the path again); a write creates a new file
//! beside the target (never through a link), confirms it the same way,
//! and renames it over the target. So a directory or link swapped in by
//! another process between the check and the I/O is refused, not
//! followed.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use shape_cli::{Failure, Files};

/// The root directory and the read cap.
#[derive(Clone, Debug)]
pub struct Workspace {
    root: PathBuf,
    max_bytes: u64,
}

fn refused(p: &Path, why: &str) -> Failure {
    Failure::Failed(format!("{}: refused: {why}", p.display()))
}

impl Workspace {
    /// A workspace on `root`, which must exist; files read are capped at
    /// `max_bytes`.
    pub fn new(root: &Path, max_bytes: u64) -> Result<Workspace, String> {
        let root = root
            .canonicalize()
            .map_err(|e| format!("--root {}: {e}", root.display()))?;
        if !root.is_dir() {
            return Err(format!("--root {}: not a directory", root.display()));
        }
        Ok(Workspace { root, max_bytes })
    }

    /// The canonical root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The read cap, in bytes.
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// `p` as a path under the root, or why not.
    pub fn resolve(&self, p: &Path) -> Result<PathBuf, Failure> {
        if p.as_os_str().is_empty() {
            return Err(refused(p, "an empty path"));
        }
        let joined = if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        };
        let mut existing = joined.clone();
        let mut rest: Vec<std::ffi::OsString> = Vec::new();
        // The longest prefix that is there (a link counts, dangling or
        // not, so that canonicalize sees it).
        while existing.symlink_metadata().is_err() {
            match existing.components().next_back() {
                Some(Component::Normal(name)) => rest.push(name.to_os_string()),
                Some(Component::ParentDir) => {
                    return Err(refused(p, "`..` below a directory that does not exist"));
                }
                _ => {}
            }
            if !existing.pop() {
                return Err(refused(p, "no part of it exists"));
            }
        }
        let canon = existing
            .canonicalize()
            .map_err(|e| refused(p, &format!("cannot resolve it ({e})")))?;
        if !canon.starts_with(&self.root) {
            return Err(refused(
                p,
                &format!("outside the workspace {}", self.root.display()),
            ));
        }
        let mut out = canon;
        for name in rest.iter().rev() {
            out.push(name);
        }
        Ok(out)
    }
}

/// `file`, opened at `path`, is the file now at `path`'s real path, and
/// that path is under `root`: the check after the open, so that a link
/// swapped in between the first check and the open is caught. On Unix
/// the opened file and the file at the real path must be the same
/// (device and inode); elsewhere std gives no file identity, and the
/// path is resolved again after the open.
fn confirm(root: &Path, path: &Path, file: &File) -> Result<(), String> {
    let real = path
        .canonicalize()
        .map_err(|e| format!("cannot resolve it after opening ({e})"))?;
    if !real.starts_with(root) {
        return Err(format!(
            "it resolved outside the workspace {} when opened",
            root.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let opened = file.metadata().map_err(|e| e.to_string())?;
        let there = std::fs::metadata(&real).map_err(|e| e.to_string())?;
        if (opened.dev(), opened.ino()) != (there.dev(), there.ino()) {
            return Err("the file changed between the check and the open".into());
        }
    }
    #[cfg(not(unix))]
    let _ = file;
    Ok(())
}

impl Workspace {
    /// Reads `full` (resolved from `shown`): opened, then confirmed.
    fn read_resolved(&self, shown: &Path, full: &Path) -> Result<Vec<u8>, Failure> {
        let fail = |e: std::io::Error| Failure::Failed(format!("{}: {e}", shown.display()));
        let file = File::open(full).map_err(fail)?;
        confirm(&self.root, full, &file).map_err(|why| refused(shown, &why))?;
        let meta = file.metadata().map_err(fail)?;
        if !meta.is_file() {
            return Err(Failure::Failed(format!("{}: not a file", shown.display())));
        }
        let over = |n: u64| {
            refused(
                shown,
                &format!(
                    "{n} bytes, over the cap of {} (--max-bytes)",
                    self.max_bytes
                ),
            )
        };
        if meta.len() > self.max_bytes {
            return Err(over(meta.len()));
        }
        // The cap holds even if the file grows while it is read.
        let mut bytes = Vec::new();
        file.take(self.max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(fail)?;
        if bytes.len() as u64 > self.max_bytes {
            return Err(over(bytes.len() as u64));
        }
        Ok(bytes)
    }

    /// Writes `full` (resolved from `shown`): into a new file beside it
    /// (created, never opened through a link), confirmed, then renamed
    /// over `full`, which replaces a link there rather than following it.
    fn write_resolved(&self, shown: &Path, full: &Path, bytes: &[u8]) -> Result<(), Failure> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let fail = |e: std::io::Error| Failure::Failed(format!("{}: {e}", shown.display()));
        let (Some(dir), Some(name)) = (full.parent(), full.file_name()) else {
            return Err(refused(shown, "not a file path"));
        };
        std::fs::create_dir_all(dir).map_err(fail)?;
        let mut tmp_name = std::ffi::OsString::from(".");
        tmp_name.push(name);
        tmp_name.push(format!(
            ".{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let tmp = dir.join(tmp_name);
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(fail)?;
        let done = confirm(&self.root, &tmp, &file)
            .map_err(|why| refused(shown, &why))
            .and_then(|()| file.write_all(bytes).map_err(fail));
        drop(file);
        if let Err(e) = done.and_then(|()| std::fs::rename(&tmp, full).map_err(fail)) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        Ok(())
    }
}

impl Files for Workspace {
    fn read(&self, path: &Path) -> Result<Vec<u8>, Failure> {
        let full = self.resolve(path)?;
        self.read_resolved(path, &full)
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), Failure> {
        let full = self.resolve(path)?;
        self.write_resolved(path, &full, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_under_the_root() {
        let base = std::env::temp_dir().join(format!("shape-mcp-ws-{}", std::process::id()));
        let root = base.join("root");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(base.join("secret.txt"), b"x").unwrap();
        let ws = Workspace::new(&root, 1 << 20).unwrap();
        assert!(ws.resolve(Path::new("sub/new/a.glb")).is_ok());
        assert!(ws.resolve(Path::new("a.glb")).is_ok());
        for bad in ["../secret.txt", "sub/../../secret.txt", "new/../../x"] {
            assert!(ws.resolve(Path::new(bad)).is_err(), "{bad}");
        }
        assert!(ws.resolve(&base.join("secret.txt")).is_err());
        assert!(ws.read(Path::new("../secret.txt")).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(base.join("secret.txt"), root.join("link")).unwrap();
            std::os::unix::fs::symlink(base.join("nowhere"), root.join("dangling")).unwrap();
            assert!(ws.read(Path::new("link")).is_err());
            assert!(ws.write(Path::new("dangling"), b"y").is_err());
            assert!(!base.join("nowhere").exists());
        }
        let small = Workspace::new(&root, 1).unwrap();
        std::fs::write(root.join("big.obj"), b"too big").unwrap();
        assert!(small.read(Path::new("big.obj")).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A directory swapped for a link after the check and before the
    /// I/O (the race, played in order): the read is refused, and the
    /// write leaves nothing outside the root.
    #[cfg(unix)]
    #[test]
    fn a_swap_after_the_check_is_refused() {
        let base = std::env::temp_dir().join(format!("shape-mcp-swap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (root, outside) = (base.join("root"), base.join("outside"));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(root.join("sub/a.obj"), b"inside").unwrap();
        std::fs::write(outside.join("a.obj"), b"secret").unwrap();
        let ws = Workspace::new(&root, 1 << 20).unwrap();
        let read = ws.resolve(Path::new("sub/a.obj")).unwrap();
        let write = ws.resolve(Path::new("sub/new.glb")).unwrap();
        std::fs::rename(root.join("sub"), root.join("old")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("sub")).unwrap();
        let r = ws.read_resolved(Path::new("sub/a.obj"), &read);
        assert!(
            r.as_ref().is_err_and(|e| e.to_string().contains("refused")),
            "{r:?}"
        );
        let w = ws.write_resolved(Path::new("sub/new.glb"), &write, b"x");
        assert!(w.is_err());
        let left: Vec<_> = std::fs::read_dir(&outside)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(left, [std::ffi::OsString::from("a.obj")]);
        assert_eq!(std::fs::read(outside.join("a.obj")).unwrap(), b"secret");
        // A link put where the target was is replaced, not written through.
        std::fs::remove_file(root.join("sub")).unwrap();
        std::fs::rename(root.join("old"), root.join("sub")).unwrap();
        let target = ws.resolve(Path::new("sub/b.glb")).unwrap();
        std::os::unix::fs::symlink(outside.join("a.obj"), root.join("sub/b.glb")).unwrap();
        ws.write_resolved(Path::new("sub/b.glb"), &target, b"glb")
            .unwrap();
        assert_eq!(std::fs::read(outside.join("a.obj")).unwrap(), b"secret");
        assert_eq!(std::fs::read(root.join("sub/b.glb")).unwrap(), b"glb");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Windows: the root is canonical (`\\?\C:\…`); paths given relative,
    /// absolute with or without the verbatim prefix, or with `/`, resolve
    /// under it; `..` out of it, another drive's path and a link out are
    /// refused. Runs in CI only (the x86-64 Windows target).
    #[cfg(windows)]
    #[test]
    fn windows_paths() {
        let base = std::env::temp_dir().join(format!("shape-mcp-win-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("root");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(base.join("secret.txt"), b"x").unwrap();
        let ws = Workspace::new(&root, 1 << 20).unwrap();
        let canon = ws.root().to_str().unwrap().to_string();
        assert!(canon.starts_with(r"\\?\"), "{canon}");
        let plain = canon.trim_start_matches(r"\\?\").to_string();
        for good in [
            "sub\\a.glb".to_string(),
            "sub/a.glb".to_string(),
            format!("{plain}\\sub\\a.glb"),
            format!("{canon}\\sub\\a.glb"),
            format!("{plain}/sub/a.glb"),
        ] {
            let r = ws.resolve(Path::new(&good));
            assert!(
                r.as_ref().is_ok_and(|p| p.starts_with(ws.root())),
                "{good}: {r:?}"
            );
        }
        for bad in [
            "..\\secret.txt".to_string(),
            "sub\\..\\..\\secret.txt".to_string(),
            format!("{}\\secret.txt", base.display()),
            r"\\?\Z:\nowhere\x.glb".to_string(),
        ] {
            assert!(ws.resolve(Path::new(&bad)).is_err(), "{bad}");
        }
        ws.write(Path::new("sub\\a.glb"), b"glb").unwrap();
        assert_eq!(
            ws.read(Path::new(&format!("{plain}\\sub\\a.glb"))).unwrap(),
            b"glb"
        );
        // Links need a privilege on Windows; test them where it is held.
        match std::os::windows::fs::symlink_file(base.join("secret.txt"), root.join("link.txt")) {
            Ok(()) => assert!(ws.read(Path::new("link.txt")).is_err()),
            Err(e) => eprintln!("windows_paths: links not tried ({e})"),
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
