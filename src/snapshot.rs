use crate::git::GitContext;
use crate::manifest::{Entry, EntryKind, Manifest, Source};
use crate::pathutil::{join_under, validate_rel_path};
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path};
use std::time::SystemTime;

#[derive(Debug, Clone)]
enum ObservedPath {
    File {
        len: u64,
        modified: Option<SystemTime>,
    },
    Symlink {
        target: String,
    },
}

pub fn capture(
    ctx: &GitContext,
    source: Source,
    message: Option<String>,
    staging_files: Option<&Path>,
) -> Result<Manifest> {
    if let Some(staging_files) = staging_files {
        fs::create_dir_all(staging_files)
            .with_context(|| format!("create staging files dir {}", staging_files.display()))?;
    }

    let initial_candidates = ctx.managed_candidates()?;
    let mut observed = BTreeMap::new();
    let mut entries = Vec::new();

    for (rel, git_mode) in &initial_candidates {
        validate_rel_path(rel)?;
        let source_path = join_under(&ctx.worktree_root, rel)?;
        let before = match fs::symlink_metadata(&source_path) {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err).with_context(|| format!("stat {}", source_path.display())),
        };
        let file_type = before.file_type();

        if file_type.is_symlink() {
            let target = fs::read_link(&source_path)
                .with_context(|| format!("read symlink target {}", source_path.display()))?;
            let target = target.into_os_string().into_string().map_err(|_| {
                anyhow::anyhow!("unsupported-file-type: symlink target is not UTF-8: {rel}")
            })?;
            observed.insert(
                rel.clone(),
                ObservedPath::Symlink {
                    target: target.clone(),
                },
            );
            entries.push(Entry {
                path: rel.clone(),
                kind: EntryKind::Symlink,
                mode: None,
                size: None,
                sha256: None,
                target: Some(target),
            });
        } else if before.is_file() {
            let before_modified = before.modified().ok();
            let hash_path = if let Some(staging_files) = staging_files {
                let dest = join_under(staging_files, rel)?;
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("create staging parent {}", parent.display()))?;
                }
                fs::copy(&source_path, &dest).with_context(|| {
                    format!(
                        "copy {} to staging {}",
                        source_path.display(),
                        dest.display()
                    )
                })?;
                let after = fs::symlink_metadata(&source_path)
                    .with_context(|| format!("re-stat {}", source_path.display()))?;
                if before.len() != after.len() || before_modified != after.modified().ok() {
                    bail!("workspace-changed: file changed while saving: {rel}");
                }
                dest
            } else {
                source_path.clone()
            };
            let (size, sha256) = sha256_file(&hash_path)?;
            observed.insert(
                rel.clone(),
                ObservedPath::File {
                    len: before.len(),
                    modified: before_modified,
                },
            );
            entries.push(Entry {
                path: rel.clone(),
                kind: EntryKind::File,
                mode: Some(file_mode(git_mode.as_deref(), &before)),
                size: Some(size),
                sha256: Some(sha256),
                target: None,
            });
        } else if before.is_dir() {
            continue;
        } else {
            bail!("unsupported-file-type: refusing to save {rel}");
        }
    }

    let final_candidates = ctx.managed_candidates()?;
    if initial_candidates.keys().collect::<Vec<_>>() != final_candidates.keys().collect::<Vec<_>>()
    {
        bail!("workspace-changed: managed path set changed while saving");
    }
    verify_observed_paths(ctx, &observed)?;

    Manifest::new(source, message, entries)
}

pub fn sha256_file(path: &Path) -> Result<(u64, String)> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 1024 * 64];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("read {}", path.display()))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((total, hex::encode(hasher.finalize())))
}

pub fn verify_materialized(root: &Path, manifest: &Manifest) -> Result<()> {
    manifest.validate()?;
    let mut expected_files = BTreeSet::new();
    for entry in &manifest.files {
        match entry.kind {
            EntryKind::File => {
                expected_files.insert(entry.path.clone());
                let path = join_under(root, &entry.path)?;
                let meta = fs::symlink_metadata(&path)
                    .with_context(|| format!("materialized file missing: {}", entry.path))?;
                if !meta.is_file() || meta.file_type().is_symlink() {
                    bail!(
                        "corrupt-checkpoint: materialized path is not a file: {}",
                        entry.path
                    );
                }
                let (size, sha256) = sha256_file(&path)?;
                if Some(size) != entry.size || Some(sha256) != entry.sha256 {
                    bail!(
                        "corrupt-checkpoint: materialized file hash mismatch: {}",
                        entry.path
                    );
                }
            }
            EntryKind::Symlink => {}
        }
    }

    let actual_files = collect_materialized_files(root)?;
    for path in &actual_files {
        if !expected_files.contains(path) {
            bail!("corrupt-checkpoint: unexpected materialized file: {path}");
        }
    }
    for path in &expected_files {
        if !actual_files.contains(path) {
            bail!("corrupt-checkpoint: expected materialized file missing: {path}");
        }
    }
    Ok(())
}

pub fn verify_worktree(ctx: &GitContext, expected: &Manifest) -> Result<()> {
    let actual = capture(
        ctx,
        Source {
            kind: "internal".to_owned(),
            operation: "verify".to_owned(),
            triggering_command: None,
            target_checkpoint: None,
        },
        None,
        None,
    )?;
    if actual.files != expected.files {
        bail!("restore-failed: final worktree does not match checkpoint manifest");
    }
    Ok(())
}

fn verify_observed_paths(
    ctx: &GitContext,
    observed: &BTreeMap<String, ObservedPath>,
) -> Result<()> {
    for (rel, expected) in observed {
        let path = join_under(&ctx.worktree_root, rel)?;
        let meta = fs::symlink_metadata(&path)
            .with_context(|| format!("workspace-changed: managed path disappeared: {rel}"))?;
        match expected {
            ObservedPath::File { len, modified } => {
                if !meta.is_file() || meta.file_type().is_symlink() {
                    bail!("workspace-changed: file type changed while saving: {rel}");
                }
                if meta.len() != *len || meta.modified().ok() != *modified {
                    bail!("workspace-changed: file changed while saving: {rel}");
                }
            }
            ObservedPath::Symlink { target } => {
                if !meta.file_type().is_symlink() {
                    bail!("workspace-changed: symlink type changed while saving: {rel}");
                }
                let current_target = fs::read_link(&path)
                    .with_context(|| format!("read symlink target {}", path.display()))?;
                let current_target =
                    current_target.into_os_string().into_string().map_err(|_| {
                        anyhow::anyhow!("unsupported-file-type: symlink target is not UTF-8: {rel}")
                    })?;
                if current_target != *target {
                    bail!("workspace-changed: symlink changed while saving: {rel}");
                }
            }
        }
    }
    Ok(())
}

fn collect_materialized_files(root: &Path) -> Result<BTreeSet<String>> {
    let mut files = BTreeSet::new();
    if !root.exists() {
        return Ok(files);
    }
    collect_materialized_files_inner(root, root, &mut files)?;
    Ok(files)
}

fn collect_materialized_files_inner(
    root: &Path,
    dir: &Path,
    files: &mut BTreeSet<String>,
) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let meta =
            fs::symlink_metadata(&path).with_context(|| format!("stat {}", path.display()))?;
        if meta.is_dir() && !meta.file_type().is_symlink() {
            collect_materialized_files_inner(root, &path, files)?;
        } else if meta.is_file() && !meta.file_type().is_symlink() {
            files.insert(path_to_manifest_rel(root, &path)?);
        } else {
            bail!(
                "corrupt-checkpoint: unsupported materialized path type: {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn path_to_manifest_rel(root: &Path, path: &Path) -> Result<String> {
    let rel = path.strip_prefix(root).with_context(|| {
        format!(
            "corrupt-checkpoint: materialized path escaped root: {}",
            path.display()
        )
    })?;
    let mut parts = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str().ok_or_else(|| {
                anyhow::anyhow!(
                    "corrupt-checkpoint: materialized path is not UTF-8: {}",
                    path.display()
                )
            })?),
            Component::CurDir => {}
            _ => bail!(
                "corrupt-checkpoint: invalid materialized path component: {}",
                path.display()
            ),
        }
    }
    let logical = parts.join("/");
    validate_rel_path(&logical)?;
    Ok(logical)
}

fn file_mode(git_mode: Option<&str>, meta: &fs::Metadata) -> String {
    if let Some(mode @ ("100644" | "100755")) = git_mode {
        return mode.to_owned();
    }
    if is_executable(meta) {
        "100755".to_owned()
    } else {
        "100644".to_owned()
    }
}

#[cfg(unix)]
fn is_executable(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &fs::Metadata) -> bool {
    false
}

pub fn set_file_mode(path: &Path, mode: Option<&str>) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bits = if mode == Some("100755") { 0o755 } else { 0o644 };
        fs::set_permissions(path, fs::Permissions::from_mode(bits))
            .with_context(|| format!("set permissions on {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}

pub fn create_symlink(target: &str, link_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link_path)
            .with_context(|| format!("create symlink {} -> {target}", link_path.display()))?;
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link_path)
            .with_context(|| format!("create symlink {} -> {target}", link_path.display()))?;
    }
    Ok(())
}
