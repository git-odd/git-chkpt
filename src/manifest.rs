use crate::pathutil::{has_case_conflict, validate_rel_path};
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const FORMAT_VERSION: u32 = 1;
pub const MANIFEST_FILE: &str = "manifest.json";
pub const FILES_DIR: &str = "files";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    pub created_at_utc: DateTime<Utc>,
    pub source: Source,
    pub message: Option<String>,
    pub files: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub kind: String,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_checkpoint: Option<String>,
}

impl Source {
    pub fn manual_save() -> Self {
        Self {
            kind: "manual".to_owned(),
            operation: "save".to_owned(),
            target_checkpoint: None,
        }
    }

    pub fn pre_restore(target_checkpoint: String) -> Self {
        Self {
            kind: "automatic".to_owned(),
            operation: "pre-restore".to_owned(),
            target_checkpoint: Some(target_checkpoint),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Entry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: EntryKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Symlink,
}

impl Manifest {
    pub fn new(source: Source, message: Option<String>, mut files: Vec<Entry>) -> Result<Self> {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let manifest = Self {
            format_version: FORMAT_VERSION,
            created_at_utc: Utc::now(),
            source,
            message,
            files,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        if self.format_version != FORMAT_VERSION {
            bail!(
                "unsupported-format: manifest version {}",
                self.format_version
            );
        }
        let mut seen = BTreeSet::new();
        let mut paths = Vec::with_capacity(self.files.len());
        let mut prev: Option<&str> = None;
        for entry in &self.files {
            validate_rel_path(&entry.path)?;
            if let Some(previous_path) = prev {
                if previous_path > entry.path.as_str() {
                    bail!("corrupt-checkpoint: manifest paths are not sorted");
                }
                if entry.path.starts_with(previous_path)
                    && entry.path.as_bytes().get(previous_path.len()) == Some(&b'/')
                {
                    bail!(
                        "corrupt-checkpoint: manifest path conflicts with ancestor: {} and {}",
                        previous_path,
                        entry.path
                    );
                }
            }
            prev = Some(&entry.path);
            if !seen.insert(entry.path.clone()) {
                bail!("corrupt-checkpoint: duplicate path {}", entry.path);
            }
            match entry.kind {
                EntryKind::File => {
                    match entry.mode.as_deref() {
                        Some("100644" | "100755") => {}
                        Some(mode) => bail!(
                            "corrupt-checkpoint: unsupported file mode {mode} for {}",
                            entry.path
                        ),
                        None => bail!(
                            "corrupt-checkpoint: file entry missing metadata: {}",
                            entry.path
                        ),
                    }
                    if entry.size.is_none() || !valid_sha256(entry.sha256.as_deref()) {
                        bail!(
                            "corrupt-checkpoint: file entry missing metadata: {}",
                            entry.path
                        );
                    }
                    if entry.target.is_some() {
                        bail!(
                            "corrupt-checkpoint: file entry has symlink target: {}",
                            entry.path
                        );
                    }
                }
                EntryKind::Symlink => {
                    match entry.target.as_deref() {
                        Some(target) if !target.contains('\0') => {}
                        Some(_) => bail!(
                            "corrupt-checkpoint: symlink target contains NUL: {}",
                            entry.path
                        ),
                        None => bail!(
                            "corrupt-checkpoint: symlink entry missing target: {}",
                            entry.path
                        ),
                    }
                    if entry.mode.is_some() || entry.size.is_some() || entry.sha256.is_some() {
                        bail!(
                            "corrupt-checkpoint: symlink entry has file metadata: {}",
                            entry.path
                        );
                    }
                }
            }
            paths.push(entry.path.clone());
        }
        if let Some((a, b)) = has_case_conflict(&paths) {
            bail!("corrupt-checkpoint: case-folding path conflict: {a} and {b}");
        }
        Ok(())
    }

    pub fn total_file_bytes(&self) -> u64 {
        self.files.iter().filter_map(|entry| entry.size).sum()
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

fn valid_sha256(value: Option<&str>) -> bool {
    matches!(value, Some(text) if text.len() == 64 && text.chars().all(|ch| ch.is_ascii_hexdigit()))
}
