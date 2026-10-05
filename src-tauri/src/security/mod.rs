use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PathSecurityError {
    #[error("workspace path must be absolute")]
    RelativeWorkspace,
    #[error("requested path escapes the workspace")]
    OutsideWorkspace,
    #[error("path does not exist")]
    Missing,
    #[error("path cannot be resolved: {0}")]
    Canonicalize(String),
}

pub fn canonical_workspace(path: &Path) -> Result<PathBuf, PathSecurityError> {
    if !path.is_absolute() {
        return Err(PathSecurityError::RelativeWorkspace);
    }
    let path = path
        .canonicalize()
        .map_err(|e| PathSecurityError::Canonicalize(e.to_string()))?;
    if !path.is_dir() {
        return Err(PathSecurityError::Missing);
    }
    Ok(path)
}

pub fn resolve_existing_path(root: &Path, requested: &Path) -> Result<PathBuf, PathSecurityError> {
    if requested.is_absolute()
        || requested.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(PathSecurityError::OutsideWorkspace);
    }
    let path = root.join(requested).canonicalize().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            PathSecurityError::Missing
        } else {
            PathSecurityError::Canonicalize(e.to_string())
        }
    })?;
    if path.starts_with(root) {
        Ok(path)
    } else {
        Err(PathSecurityError::OutsideWorkspace)
    }
}

pub fn resolve_write_path(root: &Path, requested: &Path) -> Result<PathBuf, PathSecurityError> {
    if requested.is_absolute()
        || requested.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(PathSecurityError::OutsideWorkspace);
    }
    let candidate = root.join(requested);
    let parent = candidate
        .parent()
        .ok_or(PathSecurityError::OutsideWorkspace)?
        .canonicalize()
        .map_err(|e| PathSecurityError::Canonicalize(e.to_string()))?;
    if !parent.starts_with(root) {
        return Err(PathSecurityError::OutsideWorkspace);
    }
    Ok(parent.join(
        candidate
            .file_name()
            .ok_or(PathSecurityError::OutsideWorkspace)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn validates_internal_and_rejects_escape() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("ok"), "x").unwrap();
        let r = canonical_workspace(d.path()).unwrap();
        assert!(resolve_existing_path(&r, Path::new("ok")).is_ok());
        assert_eq!(
            resolve_existing_path(&r, Path::new("../ok")),
            Err(PathSecurityError::OutsideWorkspace)
        );
        assert_eq!(
            resolve_existing_path(&r, Path::new("/etc/passwd")),
            Err(PathSecurityError::OutsideWorkspace)
        );
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let o = tempfile::tempdir().unwrap();
        fs::write(o.path().join("secret"), "x").unwrap();
        symlink(o.path(), d.path().join("out")).unwrap();
        let r = canonical_workspace(d.path()).unwrap();
        assert_eq!(
            resolve_existing_path(&r, Path::new("out/secret")),
            Err(PathSecurityError::OutsideWorkspace)
        );
    }
}
