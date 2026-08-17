use crate::pathutil::normalize_git_path;
use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct GitContext {
    pub worktree_root: PathBuf,
    pub git_dir: PathBuf,
}

impl GitContext {
    pub fn discover() -> Result<Self> {
        let cwd = std::env::current_dir().context("read current directory")?;
        let is_bare = match git_stdout(None, ["rev-parse", "--is-bare-repository"]) {
            Ok(value) => value,
            Err(_) => bail!("not-a-git-worktree: git-chkpt requires a Git worktree"),
        };
        if is_bare.trim() == "true" {
            bail!("bare-repository: git-chkpt requires a non-bare Git worktree");
        }

        let root_raw = git_stdout(None, ["rev-parse", "--show-toplevel"])
            .context("not-a-git-worktree: failed to resolve worktree root")?;
        let git_dir_raw = git_stdout(None, ["rev-parse", "--git-dir"])
            .context("failed to resolve worktree Git administrative directory")?;

        let worktree_root = resolve_from(&cwd, root_raw.trim())?;
        let git_dir = resolve_from(&cwd, git_dir_raw.trim())?;

        Ok(Self {
            worktree_root,
            git_dir,
        })
    }

    pub fn managed_candidates(&self) -> Result<BTreeMap<String, Option<String>>> {
        let mut candidates = BTreeMap::new();
        let mut boundaries = BTreeSet::new();

        let tracked = git_output_bytes(&self.worktree_root, ["ls-files", "--stage", "-z"])?;
        for record in tracked
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
        {
            let text =
                String::from_utf8(record.to_vec()).context("git ls-files output is not UTF-8")?;
            let (meta, path) = text
                .split_once('\t')
                .with_context(|| format!("unexpected git ls-files --stage record: {text:?}"))?;
            let mode = meta
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned();
            let path = normalize_git_path(path)?;
            if mode == "160000" {
                boundaries.insert(path);
            } else if !is_inside_boundary(&path, &boundaries) {
                candidates.insert(path, Some(mode));
            }
        }

        let untracked = git_output_bytes(
            &self.worktree_root,
            ["ls-files", "-o", "--exclude-standard", "-z"],
        )?;
        for record in untracked
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
        {
            let text =
                String::from_utf8(record.to_vec()).context("git ls-files output is not UTF-8")?;
            let path = normalize_git_path(text.trim_end_matches('/'))?;
            if is_inside_boundary(&path, &boundaries)
                || has_nested_git_boundary(&self.worktree_root, &path)
            {
                continue;
            }
            candidates.entry(path).or_insert(None);
        }

        Ok(candidates)
    }
}

fn resolve_from(cwd: &Path, raw: &str) -> Result<PathBuf> {
    let path = PathBuf::from(raw);
    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    Ok(normalize_absolute_lexical(&path))
}

fn normalize_absolute_lexical(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn git_stdout<const N: usize>(cwd: Option<&Path>, args: [&str; N]) -> Result<String> {
    let output = git_output(cwd, args)?;
    String::from_utf8(output).context("git output is not UTF-8")
}

fn git_output_bytes<const N: usize>(cwd: &Path, args: [&str; N]) -> Result<Vec<u8>> {
    git_output(Some(cwd), args)
}

fn git_output<const N: usize>(cwd: Option<&Path>, args: [&str; N]) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output().context("failed to execute git")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git failed: {stderr}");
    }
    Ok(output.stdout)
}

fn is_inside_boundary(path: &str, boundaries: &BTreeSet<String>) -> bool {
    boundaries
        .iter()
        .any(|boundary| path == boundary || path.starts_with(&format!("{boundary}/")))
}

fn has_nested_git_boundary(root: &Path, rel: &str) -> bool {
    let mut current = root.to_path_buf();
    let mut parts = rel.split('/').peekable();
    while let Some(part) = parts.next() {
        current.push(part);
        if current.join(".git").exists() {
            return true;
        }
        if parts.peek().is_none() {
            break;
        }
    }
    false
}
