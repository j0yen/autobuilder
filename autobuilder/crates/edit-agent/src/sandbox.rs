//! Path-prefix sandbox: every file the edit-agent touches must canonicalize
//! to a descendant of the configured root. Symlink-following is intentional
//! — `Path::canonicalize` resolves them, so a symlink pointing outside root
//! is rejected the same as a literal traversal would be.
//!
//! What this DOES NOT protect against: the `bash` tool runs subprocesses
//! with the parent's environment and full filesystem access. The sandbox
//! gates *direct* file I/O performed by the `read_file` / `write_file` /
//! `edit_file` tools; if a bash command does `rm -rf ~`, this module
//! cannot stop it.
//! That tradeoff is documented in the crate-level docs and the PRD.

use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

/// Resolved sandbox: a canonicalized absolute root. Paths checked through
/// this struct are themselves canonicalized before the prefix test.
#[derive(Debug, Clone)]
pub struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    /// Build a sandbox rooted at `root`. The root must exist and
    /// canonicalize (i.e. resolve symlinks) successfully.
    ///
    /// # Errors
    /// Returns an error if `root` is missing or not canonicalizable.
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self> {
        let root = root
            .as_ref()
            .canonicalize()
            .with_context(|| format!("sandbox root {} not found", root.as_ref().display()))?;
        Ok(Self { root })
    }

    /// Return the absolute root the sandbox confines to.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve `relative_or_absolute` against the sandbox root, then check
    /// the canonical form is inside. For `write_file` (where the target
    /// may not yet exist), canonicalize the parent and re-join the basename.
    ///
    /// # Errors
    /// Returns an error if the resolved path escapes the sandbox or if
    /// canonicalization fails for an unexpected reason.
    pub fn resolve_for_read<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf> {
        let candidate = self.absolute(path.as_ref());
        let canonical = candidate
            .canonicalize()
            .with_context(|| format!("resolving {} for read", candidate.display()))?;
        self.assert_inside(&canonical)?;
        Ok(canonical)
    }

    /// Resolve a write target. The file itself may not exist yet, but its
    /// parent directory must.
    ///
    /// # Errors
    /// Returns an error if the parent is missing, escapes the sandbox, or
    /// canonicalization fails.
    pub fn resolve_for_write<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf> {
        let candidate = self.absolute(path.as_ref());
        let parent = candidate.parent().ok_or_else(|| {
            anyhow!("write path {} has no parent directory", candidate.display())
        })?;
        let parent_canon = parent
            .canonicalize()
            .with_context(|| format!("parent {} does not exist", parent.display()))?;
        self.assert_inside(&parent_canon)?;
        let basename = candidate
            .file_name()
            .ok_or_else(|| anyhow!("write path {} missing file name", candidate.display()))?;
        Ok(parent_canon.join(basename))
    }

    fn absolute(&self, p: &Path) -> PathBuf {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        }
    }

    fn assert_inside(&self, canonical: &Path) -> Result<()> {
        if canonical.starts_with(&self.root) {
            Ok(())
        } else {
            Err(anyhow!(
                "path {} escapes sandbox rooted at {}",
                canonical.display(),
                self.root.display()
            ))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn resolve_for_read_accepts_in_sandbox_file() {
        let tmp = TempDir::new().unwrap();
        let f = tmp.path().join("ok.txt");
        std::fs::write(&f, "hi").unwrap();
        let s = Sandbox::new(tmp.path()).unwrap();
        let resolved = s.resolve_for_read("ok.txt").unwrap();
        assert_eq!(resolved, f.canonicalize().unwrap());
    }

    #[test]
    fn resolve_for_read_rejects_traversal() {
        let outer = TempDir::new().unwrap();
        let inner = outer.path().join("inside");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(outer.path().join("escape.txt"), "x").unwrap();
        let s = Sandbox::new(&inner).unwrap();
        let err = s.resolve_for_read("../escape.txt").unwrap_err();
        assert!(
            format!("{err}").contains("escapes sandbox"),
            "expected sandbox error, got: {err}"
        );
    }

    #[test]
    fn resolve_for_write_accepts_new_file_in_sandbox() {
        let tmp = TempDir::new().unwrap();
        let s = Sandbox::new(tmp.path()).unwrap();
        let resolved = s.resolve_for_write("new.txt").unwrap();
        assert!(resolved.starts_with(tmp.path().canonicalize().unwrap()));
    }

    #[test]
    fn resolve_for_write_rejects_parent_outside_sandbox() {
        let outer = TempDir::new().unwrap();
        let inner = outer.path().join("inside");
        std::fs::create_dir_all(&inner).unwrap();
        let s = Sandbox::new(&inner).unwrap();
        let err = s.resolve_for_write("../escape.txt").unwrap_err();
        assert!(
            format!("{err}").contains("escapes sandbox"),
            "expected sandbox error, got: {err}"
        );
    }
}
