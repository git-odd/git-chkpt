use crate::manifest::{FILES_DIR, MANIFEST_FILE, Manifest};
use crate::pathutil::{join_under, normalize_git_path};
use crate::snapshot::verify_materialized;
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::NamedTempFile;

#[derive(Debug, Clone)]
pub struct Store {
    pub base: PathBuf,
    pub repo: PathBuf,
    pub checkout: PathBuf,
    pub staging: PathBuf,
    pub transactions: PathBuf,
    pub lock: PathBuf,
    version: PathBuf,
    deleted: PathBuf,
}

impl Store {
    pub fn new(git_dir: &Path) -> Self {
        let base = git_dir.join("git-chkpt");
        Self {
            repo: base.join("repository.fossil"),
            checkout: base.join("checkout"),
            staging: base.join("staging"),
            transactions: base.join("transactions"),
            lock: base.join("lock"),
            version: base.join("version"),
            deleted: base.join("deleted.json"),
            base,
        }
    }

    pub fn create_base(&self) -> Result<()> {
        fs::create_dir_all(&self.base)
            .with_context(|| format!("create {}", self.base.display()))?;
        Ok(())
    }

    pub fn is_initialized(&self) -> bool {
        self.repo.is_file() && self.checkout_marker_exists()
    }

    pub fn ensure_initialized(&self) -> Result<()> {
        self.create_base()?;
        fs::create_dir_all(&self.transactions)
            .with_context(|| format!("create {}", self.transactions.display()))?;

        if !self.repo.exists() {
            fossil(
                None,
                [
                    OsStr::new("init"),
                    OsStr::new("--admin-user"),
                    OsStr::new("git-chkpt"),
                    OsStr::new("--project-name"),
                    OsStr::new("git-chkpt"),
                    OsStr::new("--project-desc"),
                    OsStr::new("local git-chkpt storage"),
                    self.repo.as_os_str(),
                ],
            )
            .context("fossil-unavailable or fossil init failed")?;
        }

        if !self.checkout_marker_exists() {
            fs::create_dir_all(&self.checkout)
                .with_context(|| format!("create {}", self.checkout.display()))?;
            fossil(
                Some(&self.checkout),
                [
                    OsStr::new("open"),
                    self.repo.as_os_str(),
                    OsStr::new("--empty"),
                    OsStr::new("--nested"),
                    OsStr::new("--nosync"),
                    OsStr::new("--force"),
                ],
            )
            .context("fossil open failed")?;
        }

        self.fossil_checkout([
            OsStr::new("settings"),
            OsStr::new("autosync"),
            OsStr::new("off"),
        ])
        .context("failed to disable Fossil autosync")?;
        fs::write(&self.version, b"1\n")
            .with_context(|| format!("write {}", self.version.display()))?;
        Ok(())
    }

    pub fn reset_staging(&self) -> Result<PathBuf> {
        if self.staging.exists() {
            fs::remove_dir_all(&self.staging)
                .with_context(|| format!("remove {}", self.staging.display()))?;
        }
        let files = self.staging.join(FILES_DIR);
        fs::create_dir_all(&files).with_context(|| format!("create {}", files.display()))?;
        Ok(files)
    }

    pub fn write_staging_manifest(&self, manifest: &Manifest) -> Result<()> {
        manifest.validate()?;
        let bytes = serde_json::to_vec_pretty(manifest).context("serialize checkpoint manifest")?;
        fs::write(self.staging.join(MANIFEST_FILE), bytes)
            .with_context(|| format!("write staging {MANIFEST_FILE}"))?;
        Ok(())
    }

    pub fn commit_staging(&self, comment: &str) -> Result<String> {
        self.clear_checkout_payload()?;
        fs::copy(
            self.staging.join(MANIFEST_FILE),
            self.checkout.join(MANIFEST_FILE),
        )
        .context("copy manifest to Fossil checkout")?;
        let staging_files = self.staging.join(FILES_DIR);
        if staging_files.exists() {
            copy_dir_contents(&staging_files, &self.checkout.join(FILES_DIR))?;
        }

        self.fossil_checkout([OsStr::new("addremove"), OsStr::new("--dotfiles")])
            .context("fossil addremove failed")?;
        self.fossil_checkout([
            OsStr::new("-U"),
            OsStr::new("git-chkpt"),
            OsStr::new("commit"),
            OsStr::new("--private"),
            OsStr::new("--allow-empty"),
            OsStr::new("--nosync"),
            OsStr::new("--no-warnings"),
            OsStr::new("--no-verify"),
            OsStr::new("--no-verify-comment"),
            OsStr::new("--no-prompt"),
            OsStr::new("--user-override"),
            OsStr::new("git-chkpt"),
            OsStr::new("-m"),
            OsStr::new(comment),
        ])
        .context("fossil commit failed")?;

        self.latest_hash()
    }

    pub fn timeline_hashes(&self) -> Result<Vec<String>> {
        if !self.repo.exists() {
            return Ok(Vec::new());
        }
        if !self.checkout_marker_exists() {
            self.ensure_initialized()?;
        }
        let out = self.fossil_checkout([
            OsStr::new("timeline"),
            OsStr::new("-n"),
            OsStr::new("0"),
            OsStr::new("--type"),
            OsStr::new("ci"),
            OsStr::new("--format"),
            OsStr::new("%H"),
            OsStr::new("-q"),
        ])?;
        let mut seen = BTreeSet::new();
        let mut hashes = Vec::new();
        for line in String::from_utf8_lossy(&out).lines() {
            let hash = line.trim();
            if hash.len() >= 8
                && hash.chars().all(|ch| ch.is_ascii_hexdigit())
                && seen.insert(hash.to_owned())
            {
                hashes.push(hash.to_owned());
            }
        }
        Ok(hashes)
    }

    pub fn read_manifest(&self, hash: &str) -> Result<Manifest> {
        let out = self.fossil_checkout([
            OsStr::new("cat"),
            OsStr::new("-r"),
            OsStr::new(hash),
            OsStr::new(MANIFEST_FILE),
        ])?;
        let manifest: Manifest =
            serde_json::from_slice(&out).context("parse checkpoint manifest")?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn read_verified_manifest(&self, hash: &str) -> Result<Manifest> {
        let manifest = self.read_manifest(hash)?;
        self.verify_checkpoint_payload(hash, &manifest)?;
        Ok(manifest)
    }

    pub fn validate_checkpoint(&self, hash: &str) -> Result<Manifest> {
        let temp = tempfile::tempdir().context("create checkpoint validation dir")?;
        self.materialize_checkpoint(hash, temp.path())
    }

    pub fn materialize_checkpoint(&self, hash: &str, dest: &Path) -> Result<Manifest> {
        let manifest = self.read_verified_manifest(hash)?;
        fs::create_dir_all(dest).with_context(|| format!("create {}", dest.display()))?;
        for entry in &manifest.files {
            if entry.kind == crate::manifest::EntryKind::File {
                let dest_path = join_under(dest, &entry.path)?;
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("create {}", parent.display()))?;
                }
                let storage_path = format!("{FILES_DIR}/{}", entry.path);
                self.fossil_checkout([
                    OsStr::new("cat"),
                    OsStr::new("-r"),
                    OsStr::new(hash),
                    OsStr::new("-o"),
                    dest_path.as_os_str(),
                    OsStr::new(&storage_path),
                ])
                .with_context(|| format!("extract {} from checkpoint {hash}", entry.path))?;
            }
        }
        verify_materialized(dest, &manifest)?;
        Ok(manifest)
    }

    pub fn load_deleted(&self) -> Result<BTreeSet<String>> {
        if !self.deleted.exists() {
            return Ok(BTreeSet::new());
        }
        let bytes =
            fs::read(&self.deleted).with_context(|| format!("read {}", self.deleted.display()))?;
        let values: Vec<String> =
            serde_json::from_slice(&bytes).context("parse deleted checkpoint list")?;
        Ok(values.into_iter().collect())
    }

    pub fn save_deleted(&self, deleted: &BTreeSet<String>) -> Result<()> {
        let values: Vec<&String> = deleted.iter().collect();
        let bytes =
            serde_json::to_vec_pretty(&values).context("serialize deleted checkpoint list")?;
        atomic_write(&self.deleted, &bytes)
    }

    fn verify_checkpoint_payload(&self, hash: &str, manifest: &Manifest) -> Result<()> {
        let actual = self.checkpoint_paths(hash)?;
        let mut expected = BTreeSet::new();
        expected.insert(MANIFEST_FILE.to_owned());
        for entry in &manifest.files {
            if entry.kind == crate::manifest::EntryKind::File {
                expected.insert(format!("{FILES_DIR}/{}", entry.path));
            }
        }
        if actual != expected {
            bail!(
                "corrupt-checkpoint: check-in payload does not match manifest for {}",
                hash
            );
        }
        Ok(())
    }

    fn checkpoint_paths(&self, hash: &str) -> Result<BTreeSet<String>> {
        let out = self.fossil_checkout([OsStr::new("ls"), OsStr::new("-r"), OsStr::new(hash)])?;
        let mut paths = BTreeSet::new();
        for line in String::from_utf8_lossy(&out).lines() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                continue;
            }
            paths.insert(normalize_git_path(line)?);
        }
        Ok(paths)
    }

    fn latest_hash(&self) -> Result<String> {
        let out = self.fossil_checkout([
            OsStr::new("timeline"),
            OsStr::new("-n"),
            OsStr::new("1"),
            OsStr::new("--type"),
            OsStr::new("ci"),
            OsStr::new("--format"),
            OsStr::new("%H"),
            OsStr::new("-q"),
        ])?;
        for line in String::from_utf8_lossy(&out).lines() {
            let hash = line.trim();
            if hash.len() >= 8 && hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
                return Ok(hash.to_owned());
            }
        }
        bail!("fossil-failed: could not determine new checkpoint ID")
    }

    fn fossil_checkout<const N: usize>(&self, args: [&OsStr; N]) -> Result<Vec<u8>> {
        fossil(Some(&self.checkout), args)
    }

    fn checkout_marker_exists(&self) -> bool {
        self.checkout.join(".fslckout").is_file() || self.checkout.join("_FOSSIL_").is_file()
    }

    fn clear_checkout_payload(&self) -> Result<()> {
        fs::create_dir_all(&self.checkout)
            .with_context(|| format!("create {}", self.checkout.display()))?;
        for entry in fs::read_dir(&self.checkout)
            .with_context(|| format!("read {}", self.checkout.display()))?
        {
            let entry = entry?;
            let name = entry.file_name();
            if name == ".fslckout" || name == "_FOSSIL_" {
                continue;
            }
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                fs::remove_dir_all(&path).with_context(|| format!("remove {}", path.display()))?;
            } else {
                fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
            }
        }
        Ok(())
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let mut tmp = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temp file in {}", parent.display()))?;
    tmp.write_all(bytes)
        .with_context(|| format!("write temp file for {}", path.display()))?;
    tmp.as_file()
        .sync_all()
        .with_context(|| format!("sync temp file for {}", path.display()))?;
    tmp.persist(path)
        .map(|_| ())
        .map_err(|err| err.error)
        .with_context(|| format!("persist {}", path.display()))
}

fn fossil<const N: usize>(cwd: Option<&Path>, args: [&OsStr; N]) -> Result<Vec<u8>> {
    let mut command = Command::new("fossil");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output().context("failed to execute fossil")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!("fossil-failed: {stderr}{stdout}");
    }
    Ok(output.stdout)
}

fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("create {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_contents(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!("copy {} to {}", src_path.display(), dst_path.display())
            })?;
        } else {
            bail!("unsupported-file-type in staging: {}", src_path.display());
        }
    }
    Ok(())
}
