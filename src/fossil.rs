use crate::manifest::{FILES_DIR, MANIFEST_FILE, Manifest};
use crate::pathutil::{join_under, normalize_git_path};
use crate::snapshot::verify_materialized;
use anyhow::{Context, Result, bail};
#[cfg(feature = "auto-fossil")]
use flate2::read::GzDecoder;
#[cfg(feature = "auto-fossil")]
use sha3::{Digest, Sha3_256};
use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
#[cfg(feature = "auto-fossil")]
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(feature = "auto-fossil")]
use std::time::Duration;
#[cfg(feature = "auto-fossil")]
use tar::Archive;
use tempfile::NamedTempFile;
#[cfg(feature = "auto-fossil")]
use zip::ZipArchive;

#[cfg(feature = "auto-fossil")]
const FOSSIL_VERSION: &str = "2.28";
#[cfg(feature = "auto-fossil")]
const FOSSIL_DOWNLOAD_BASE: &str = "https://fossil-scm.org/home/uv";

#[cfg(feature = "auto-fossil")]
#[derive(Debug, Clone, Copy)]
struct FossilArtifact {
    file_name: &'static str,
    sha3_256: &'static str,
    binary_name: &'static str,
    target_label: &'static str,
}

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
    let fossil = fossil_program()?;
    let mut command = Command::new(&fossil);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to execute Fossil sidecar {}", fossil.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!("fossil-failed: {stderr}{stdout}");
    }
    Ok(output.stdout)
}

fn fossil_program() -> Result<PathBuf> {
    if let Some(value) = env::var_os("GIT_CHKPT_FOSSIL")
        && !value.is_empty()
    {
        return Ok(PathBuf::from(value));
    }

    if let Some(sidecar) = packaged_fossil_sidecar()? {
        return Ok(sidecar);
    }

    #[cfg(feature = "auto-fossil")]
    if let Some(auto) = auto_fossil()? {
        return Ok(auto);
    }

    let current_exe = env::current_exe().context("locate current git-chkpt executable")?;
    let exe_dir = current_exe
        .parent()
        .context("current git-chkpt executable has no parent directory")?;
    let expected = fossil_sidecar_candidates(exe_dir)
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "fossil-unavailable: packaged Fossil sidecar not found and automatic Fossil provisioning is unavailable for this target; expected one of: {expected}, or set GIT_CHKPT_FOSSIL"
    )
}

fn packaged_fossil_sidecar() -> Result<Option<PathBuf>> {
    let current_exe = env::current_exe().context("locate current git-chkpt executable")?;
    let exe_dir = current_exe
        .parent()
        .context("current git-chkpt executable has no parent directory")?;
    for candidate in fossil_sidecar_candidates(exe_dir) {
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

#[cfg(feature = "auto-fossil")]
fn auto_fossil() -> Result<Option<PathBuf>> {
    let Some(artifact) = fossil_artifact_for_target() else {
        return Ok(None);
    };
    let cache_dir = auto_fossil_cache_dir()
        .join(FOSSIL_VERSION)
        .join(artifact.target_label);
    fs::create_dir_all(&cache_dir).with_context(|| format!("create {}", cache_dir.display()))?;
    let binary_path = cache_dir.join(fossil_binary_name());
    if binary_path.is_file() {
        return Ok(Some(binary_path));
    }

    let archive_path = cache_dir.join(artifact.file_name);
    let url = format!("{FOSSIL_DOWNLOAD_BASE}/{}", artifact.file_name);
    download_if_needed(&url, &archive_path, artifact.sha3_256)?;
    extract_fossil_binary(&archive_path, artifact.binary_name, &binary_path)?;
    Ok(Some(binary_path))
}

#[cfg(feature = "auto-fossil")]
fn fossil_artifact_for_target() -> Option<FossilArtifact> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(FossilArtifact {
            file_name: "fossil-linux-x64-2.28.tar.gz",
            sha3_256: "cbd89e653e1b797802f2ee5bb55d6ad4959291ec6d6eb192c79ff62d9a224c33",
            binary_name: "fossil",
            target_label: "linux-x64",
        })
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(FossilArtifact {
            file_name: "fossil-mac-arm-2.28.tar.gz",
            sha3_256: "7b93271bc54345bbe26a3406a731f6e8448cab933d70435186dbbf3e17cfd522",
            binary_name: "fossil",
            target_label: "mac-arm",
        })
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some(FossilArtifact {
            file_name: "fossil-mac-x64-2.28.tar.gz",
            sha3_256: "6451b46d57e1390e18b3f2a967adb5a7daad9216b7126768fbd3682a726b9c57",
            binary_name: "fossil",
            target_label: "mac-x64",
        })
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some(FossilArtifact {
            file_name: "fossil-w64-2.28.zip",
            sha3_256: "1052b02b0594358d701170f8e2db7f948513cc44f44118a91369ab3f93641482",
            binary_name: "fossil.exe",
            target_label: "windows-x64",
        })
    } else if cfg!(all(target_os = "windows", target_arch = "x86")) {
        Some(FossilArtifact {
            file_name: "fossil-w32-2.28.zip",
            sha3_256: "ace95312ff939b52208b7e6ce4864bf0c4f008a971117798c8153e19e1fd0251",
            binary_name: "fossil.exe",
            target_label: "windows-x86",
        })
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        Some(FossilArtifact {
            file_name: "fossil-win-arm-2.28.zip",
            sha3_256: "8901958fc561bea738565efbfb51928740a094c659bca875b35d5d2ca25d4ed8",
            binary_name: "fossil.exe",
            target_label: "windows-arm64",
        })
    } else {
        None
    }
}

#[cfg(feature = "auto-fossil")]
fn auto_fossil_cache_dir() -> PathBuf {
    if let Some(path) = env::var_os("GIT_CHKPT_FOSSIL_RUNTIME_CACHE") {
        return PathBuf::from(path);
    }
    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(path).join("git-chkpt").join("fossil");
    }
    if cfg!(windows)
        && let Some(path) = env::var_os("LOCALAPPDATA")
    {
        return PathBuf::from(path).join("git-chkpt").join("fossil");
    }
    if let Some(path) = env::var_os("HOME") {
        return PathBuf::from(path)
            .join(".cache")
            .join("git-chkpt")
            .join("fossil");
    }
    env::temp_dir().join("git-chkpt").join("fossil")
}

#[cfg(feature = "auto-fossil")]
fn download_if_needed(url: &str, archive_path: &Path, expected_sha3: &str) -> Result<()> {
    if archive_path.is_file() {
        verify_sha3(archive_path, expected_sha3).with_context(|| {
            format!(
                "cached Fossil archive failed verification: {}",
                archive_path.display()
            )
        })?;
        return Ok(());
    }

    let tmp_path = archive_path.with_extension(format!("tmp-{}", std::process::id()));
    let mut last_err = None;
    for _attempt in 1..=3 {
        match download_once(url, &tmp_path) {
            Ok(()) => {
                verify_sha3(&tmp_path, expected_sha3).with_context(|| {
                    format!("downloaded Fossil archive failed verification: {url}")
                })?;
                fs::rename(&tmp_path, archive_path)
                    .with_context(|| format!("persist {}", archive_path.display()))?;
                return Ok(());
            }
            Err(err) => {
                let _ = fs::remove_file(&tmp_path);
                last_err = Some(err);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("unknown download error")))
        .with_context(|| format!("download Fossil sidecar from {url}"))
}

#[cfg(feature = "auto-fossil")]
fn download_once(url: &str, tmp_path: &Path) -> Result<()> {
    let response = ureq::get(url)
        .timeout(Duration::from_secs(300))
        .call()
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;
    let mut reader = response.into_reader();
    let mut tmp =
        fs::File::create(tmp_path).with_context(|| format!("create {}", tmp_path.display()))?;
    io::copy(&mut reader, &mut tmp).with_context(|| format!("write {}", tmp_path.display()))?;
    Ok(())
}

#[cfg(feature = "auto-fossil")]
fn verify_sha3(path: &Path, expected_sha3: &str) -> Result<()> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha3_256::new();
    let mut buffer = [0_u8; 1024 * 64];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = hex::encode(hasher.finalize());
    if actual != expected_sha3 {
        bail!("expected SHA3-256 {expected_sha3}, got {actual}");
    }
    Ok(())
}

#[cfg(feature = "auto-fossil")]
fn extract_fossil_binary(archive_path: &Path, binary_name: &str, dest: &Path) -> Result<()> {
    if archive_path.extension().and_then(|value| value.to_str()) == Some("zip") {
        extract_from_zip(archive_path, binary_name, dest)?;
    } else {
        extract_from_tar_gz(archive_path, binary_name, dest)?;
    }
    set_executable(dest)
}

#[cfg(feature = "auto-fossil")]
fn extract_from_zip(archive_path: &Path, binary_name: &str, dest: &Path) -> Result<()> {
    let file =
        fs::File::open(archive_path).with_context(|| format!("open {}", archive_path.display()))?;
    let mut archive = ZipArchive::new(file).context("read Fossil zip archive")?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).context("read Fossil zip entry")?;
        if entry.is_dir() || !entry.name().replace('\\', "/").ends_with(binary_name) {
            continue;
        }
        let mut out =
            fs::File::create(dest).with_context(|| format!("create {}", dest.display()))?;
        io::copy(&mut entry, &mut out).context("extract Fossil binary from zip")?;
        return Ok(());
    }
    bail!("Fossil zip archive did not contain {binary_name}")
}

#[cfg(feature = "auto-fossil")]
fn extract_from_tar_gz(archive_path: &Path, binary_name: &str, dest: &Path) -> Result<()> {
    let bytes =
        fs::read(archive_path).with_context(|| format!("read {}", archive_path.display()))?;
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = Archive::new(decoder);
    for entry in archive.entries().context("read Fossil tar.gz entries")? {
        let mut entry = entry.context("read Fossil tar.gz entry")?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().context("read Fossil tar.gz entry path")?;
        if path.file_name().and_then(|value| value.to_str()) != Some(binary_name) {
            continue;
        }
        let mut out =
            fs::File::create(dest).with_context(|| format!("create {}", dest.display()))?;
        io::copy(&mut entry, &mut out).context("extract Fossil binary from tar.gz")?;
        return Ok(());
    }
    bail!("Fossil tar.gz archive did not contain {binary_name}")
}

#[cfg(all(feature = "auto-fossil", unix))]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .with_context(|| format!("stat {}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).with_context(|| format!("chmod {}", path.display()))
}

#[cfg(all(feature = "auto-fossil", not(unix)))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn fossil_sidecar_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    let name = fossil_binary_name();
    [
        exe_dir.join(name),
        exe_dir.join("bin").join(name),
        exe_dir.join("sidecar").join(name),
        exe_dir.join("sidecars").join(name),
    ]
    .into_iter()
    .collect()
}

fn fossil_binary_name() -> &'static str {
    if cfg!(windows) {
        "fossil.exe"
    } else {
        "fossil"
    }
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
