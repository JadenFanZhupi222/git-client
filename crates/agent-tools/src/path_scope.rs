use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathScopeError {
    Root,
    InvalidPath,
    ForbiddenPath,
    OutsideRoot,
    Missing,
    NotFile,
    NotDirectory,
    Io,
}

#[derive(Debug, Clone)]
pub struct PathScope {
    root: PathBuf,
    deny_git: bool,
}

impl PathScope {
    pub fn new(root: &Path, deny_git: bool) -> Result<Self, PathScopeError> {
        let root = root.canonicalize().map_err(|_| PathScopeError::Root)?;
        if !root.is_dir() {
            return Err(PathScopeError::Root);
        }
        Ok(Self { root, deny_git })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn existing_file(&self, relative: &str) -> Result<PathBuf, PathScopeError> {
        let candidate = self.existing(relative)?;
        if candidate.is_file() {
            Ok(candidate)
        } else {
            Err(PathScopeError::NotFile)
        }
    }

    pub fn existing_directory(&self, relative: &str) -> Result<PathBuf, PathScopeError> {
        if relative.is_empty() || relative == "." {
            return Ok(self.root.clone());
        }
        let candidate = self.existing(relative)?;
        if candidate.is_dir() {
            Ok(candidate)
        } else {
            Err(PathScopeError::NotDirectory)
        }
    }

    pub fn write_target(&self, relative: &str) -> Result<PathBuf, PathScopeError> {
        let components = self.components(relative, false)?;
        let file_name = components.last().ok_or(PathScopeError::InvalidPath)?;
        let mut parent = self.root.clone();
        for component in &components[..components.len() - 1] {
            parent.push(component);
            if parent.exists() {
                let metadata =
                    std::fs::symlink_metadata(&parent).map_err(|_| PathScopeError::Io)?;
                if metadata.file_type().is_symlink() {
                    let canonical = parent.canonicalize().map_err(|_| PathScopeError::Io)?;
                    self.ensure_inside(&canonical)?;
                    parent = canonical;
                } else if !metadata.is_dir() {
                    return Err(PathScopeError::NotDirectory);
                }
            } else {
                std::fs::create_dir(&parent).map_err(|_| PathScopeError::Io)?;
            }
            let canonical = parent.canonicalize().map_err(|_| PathScopeError::Io)?;
            self.ensure_inside(&canonical)?;
            parent = canonical;
        }
        let target = parent.join(file_name);
        if target.exists() {
            let canonical = target.canonicalize().map_err(|_| PathScopeError::Io)?;
            self.ensure_inside(&canonical)?;
            if canonical.is_dir() {
                return Err(PathScopeError::NotFile);
            }
            Ok(canonical)
        } else {
            self.ensure_inside(&parent)?;
            Ok(target)
        }
    }

    pub fn relative_display(&self, path: &Path) -> Result<String, PathScopeError> {
        path.strip_prefix(&self.root)
            .map_err(|_| PathScopeError::OutsideRoot)
            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
    }

    fn existing(&self, relative: &str) -> Result<PathBuf, PathScopeError> {
        let components = self.components(relative, true)?;
        let candidate = components
            .iter()
            .fold(self.root.clone(), |mut path, component| {
                path.push(component);
                path
            });
        let canonical = candidate
            .canonicalize()
            .map_err(|_| PathScopeError::Missing)?;
        self.ensure_inside(&canonical)?;
        Ok(canonical)
    }

    fn components<'a>(
        &self,
        relative: &'a str,
        allow_empty: bool,
    ) -> Result<Vec<&'a OsStr>, PathScopeError> {
        if relative.contains('\0') || (!allow_empty && relative.is_empty()) {
            return Err(PathScopeError::InvalidPath);
        }
        if relative.is_empty() {
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        for component in Path::new(relative).components() {
            let Component::Normal(component) = component else {
                return Err(PathScopeError::InvalidPath);
            };
            if self.deny_git && component.eq_ignore_ascii_case(OsStr::new(".git")) {
                return Err(PathScopeError::ForbiddenPath);
            }
            output.push(component);
        }
        if output.is_empty() && !allow_empty {
            return Err(PathScopeError::InvalidPath);
        }
        Ok(output)
    }

    fn ensure_inside(&self, path: &Path) -> Result<(), PathScopeError> {
        if path.starts_with(&self.root) {
            Ok(())
        } else {
            Err(PathScopeError::OutsideRoot)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_absolute_git_and_outside_symlink() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();
        let scope = PathScope::new(root.path(), true).unwrap();
        assert_eq!(
            scope.existing_file("../secret").unwrap_err(),
            PathScopeError::InvalidPath
        );
        assert_eq!(
            scope.existing_file("/secret").unwrap_err(),
            PathScopeError::InvalidPath
        );
        #[cfg(windows)]
        assert_eq!(
            scope.existing_file("C:\\secret").unwrap_err(),
            PathScopeError::InvalidPath
        );
        assert_eq!(
            scope.existing_directory(".git").unwrap_err(),
            PathScopeError::ForbiddenPath
        );
        assert_eq!(scope.existing_directory(".").unwrap(), scope.root());
    }

    #[test]
    fn rejects_a_symlink_that_resolves_outside_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let outside_file = outside.path().join("secret.txt");
        std::fs::write(&outside_file, "secret").unwrap();
        let link = root.path().join("link.txt");
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside_file, &link).is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&outside_file, &link).is_ok();
        if !linked {
            return;
        }
        let scope = PathScope::new(root.path(), true).unwrap();
        assert_eq!(
            scope.existing_file("link.txt").unwrap_err(),
            PathScopeError::OutsideRoot
        );
        assert_eq!(
            scope.write_target("link.txt").unwrap_err(),
            PathScopeError::OutsideRoot
        );
    }

    #[test]
    fn creates_nested_write_parents_inside_root() {
        let root = tempfile::tempdir().unwrap();
        let scope = PathScope::new(root.path(), true).unwrap();
        let target = scope.write_target("nested/file.txt").unwrap();
        assert!(target.starts_with(scope.root()));
        assert!(target.parent().unwrap().is_dir());
    }
}
