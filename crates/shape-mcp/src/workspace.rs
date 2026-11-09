//! The workspace: the one directory the server reads and writes.
//!
//! Entry: [`Workspace`]. Every path a tool takes is relative to the root
//! (or absolute inside it). A path is resolved by canonicalizing its
//! longest existing prefix (which follows symbolic links) and appending
//! the rest; the result must lie under the canonical root, and the rest
//! may hold no `..`. A dangling symbolic link is refused, so a write
//! never goes through one. Files read are capped at `max_bytes`.
//!
//! The check runs before every read and write; a link swapped in between
//! by another process is not guarded against (the server trusts its own
//! machine, not its caller's paths).

use std::path::{Component, Path, PathBuf};

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

impl Files for Workspace {
    fn read(&self, path: &Path) -> Result<Vec<u8>, Failure> {
        let full = self.resolve(path)?;
        let meta = std::fs::metadata(&full)
            .map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))?;
        if !meta.is_file() {
            return Err(Failure::Failed(format!("{}: not a file", path.display())));
        }
        if meta.len() > self.max_bytes {
            return Err(refused(
                path,
                &format!(
                    "{} bytes, over the cap of {} (--max-bytes)",
                    meta.len(),
                    self.max_bytes
                ),
            ));
        }
        std::fs::read(&full).map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), Failure> {
        let full = self.resolve(path)?;
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))?;
        }
        std::fs::write(&full, bytes)
            .map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))
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
}
