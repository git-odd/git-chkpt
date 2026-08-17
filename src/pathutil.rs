use anyhow::{Context, Result, bail};
use std::path::{Component, Path, PathBuf};

pub fn normalize_git_path(raw: &str) -> Result<String> {
    let path = raw.replace('\\', "/");
    validate_rel_path(&path)?;
    Ok(path)
}

pub fn validate_rel_path(path: &str) -> Result<()> {
    if path.is_empty() {
        bail!("empty path in checkpoint manifest");
    }
    if path.contains('\0') {
        bail!("path contains NUL: {path:?}");
    }
    let p = Path::new(path);
    if p.is_absolute()
        || path.starts_with('/')
        || looks_like_windows_absolute(path)
        || path.starts_with("//")
    {
        bail!("absolute path is not allowed: {path}");
    }
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            bail!("unsafe path component in {path}");
        }
    }
    if path == ".git" || path.starts_with(".git/") {
        bail!(".git is outside the managed universe");
    }
    Ok(())
}

pub fn join_under(root: &Path, rel: &str) -> Result<PathBuf> {
    validate_rel_path(rel)?;
    let mut out = root.to_path_buf();
    for part in rel.split('/') {
        out.push(part);
    }
    Ok(out)
}

pub fn is_within_lexical(root: &Path, candidate: &Path) -> bool {
    let root_components = normalize_components(root);
    let candidate_components = normalize_components(candidate);
    candidate_components.starts_with(&root_components)
}

pub fn ensure_no_symlink_ancestors(root: &Path, rel: &str) -> Result<()> {
    validate_rel_path(rel)?;
    let mut current = root.to_path_buf();
    let mut parts = rel.split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            break;
        }
        current.push(part);
        if let Ok(meta) = std::fs::symlink_metadata(&current) {
            if meta.file_type().is_symlink() {
                bail!(
                    "refusing to write through symlink ancestor: {}",
                    current.display()
                );
            }
            if !meta.is_dir() {
                bail!("path ancestor is not a directory: {}", current.display());
            }
        }
    }
    Ok(())
}

pub fn has_case_conflict(paths: &[String]) -> Option<(String, String)> {
    let mut seen = std::collections::BTreeMap::<String, String>::new();
    for path in paths {
        let folded = path.to_lowercase();
        if let Some(prev) = seen.insert(folded, path.clone())
            && prev != *path
        {
            return Some((prev, path.clone()));
        }
    }
    None
}

pub fn remove_empty_parent_dirs(root: &Path, rel: &str) -> Result<()> {
    validate_rel_path(rel)?;
    let path = join_under(root, rel)?;
    let mut dir = path.parent().map(Path::to_path_buf);
    while let Some(current) = dir {
        if current == root || !is_within_lexical(root, &current) {
            break;
        }
        match std::fs::remove_dir(&current) {
            Ok(()) => dir = current.parent().map(Path::to_path_buf),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                dir = current.parent().map(Path::to_path_buf)
            }
            Err(err) if err.kind() == std::io::ErrorKind::DirectoryNotEmpty => break,
            Err(err) => {
                return Err(err).with_context(|| format!("remove empty dir {}", current.display()));
            }
        }
    }
    Ok(())
}

fn looks_like_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[1] == b':' && (bytes[2] == b'/' || bytes[2] == b'\\')
}

fn normalize_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Prefix(prefix) => Some(prefix.as_os_str().to_string_lossy().to_lowercase()),
            Component::RootDir => Some(String::from("/")),
            Component::Normal(part) => Some(part.to_string_lossy().to_lowercase()),
            Component::CurDir => None,
            Component::ParentDir => Some(String::from("..")),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_paths() {
        assert!(validate_rel_path("src/lib.rs").is_ok());
        assert!(validate_rel_path("../x").is_err());
        assert!(validate_rel_path("/x").is_err());
        assert!(validate_rel_path("C:/x").is_err());
        assert!(validate_rel_path(".git/config").is_err());
    }
}
