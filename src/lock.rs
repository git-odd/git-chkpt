use anyhow::{Context, Result, bail};
use chrono::{SecondsFormat, Utc};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::path::Path;

pub struct RepoLock {
    file: File,
}

impl RepoLock {
    pub fn acquire_for(path: &Path, operation: &str) -> Result<Self> {
        let file = open_lock_file(path)?;
        if let Err(err) = file.try_lock_exclusive() {
            let holder = std::fs::read_to_string(path).unwrap_or_default();
            bail!(
                "repository-busy: could not acquire {}; holder: {} ({err})",
                path.display(),
                one_line_holder(holder)
            );
        }
        file.set_len(0).context("truncate lock metadata")?;
        use std::io::Write;
        writeln!(&file, "pid={}", std::process::id()).context("write lock metadata")?;
        writeln!(&file, "operation={operation}").context("write lock metadata")?;
        writeln!(
            &file,
            "started_at_utc={}",
            Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
        )
        .context("write lock metadata")?;
        Ok(Self { file })
    }

    pub fn acquire_shared(path: &Path) -> Result<Self> {
        let file = open_lock_file(path)?;
        if let Err(err) = file.try_lock_shared() {
            let holder = std::fs::read_to_string(path).unwrap_or_default();
            bail!(
                "repository-busy: could not acquire shared lock {}; holder: {} ({err})",
                path.display(),
                one_line_holder(holder)
            );
        }
        Ok(Self { file })
    }
}

impl Drop for RepoLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn open_lock_file(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create lock parent {}", parent.display()))?;
    }
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .with_context(|| format!("open lock file {}", path.display()))
}

fn one_line_holder(holder: String) -> String {
    let text = holder
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if text.is_empty() {
        "unknown".to_owned()
    } else {
        text
    }
}
