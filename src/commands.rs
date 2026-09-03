use crate::cli::{Cli, Command, join_message};
use crate::fossil::Store;
use crate::git::GitContext;
use crate::lock::RepoLock;
use crate::manifest::{Entry, EntryKind, Manifest, Source};
use crate::pathutil::{ensure_no_symlink_ancestors, join_under, remove_empty_parent_dirs};
use crate::snapshot::{capture, create_symlink, set_file_mode, verify_worktree};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tempfile::{NamedTempFile, TempDir};

#[derive(Debug, Clone)]
struct Checkpoint {
    id: String,
    manifest: Manifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RestoreJournal {
    format_version: u32,
    status: JournalStatus,
    target_checkpoint: String,
    pre_restore_checkpoint: Option<String>,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
    stage: String,
    diagnostic: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum JournalStatus {
    InProgress,
    Complete,
    Failed,
}

impl RestoreJournal {
    fn new(target_checkpoint: String) -> Self {
        let now = Utc::now();
        Self {
            format_version: 1,
            status: JournalStatus::InProgress,
            target_checkpoint,
            pre_restore_checkpoint: None,
            created_at_utc: now,
            updated_at_utc: now,
            stage: "target-validated".to_owned(),
            diagnostic: None,
        }
    }

    fn set_stage(&mut self, stage: &str) {
        self.stage = stage.to_owned();
        self.updated_at_utc = Utc::now();
    }

    fn set_pre_restore(&mut self, id: String) {
        self.pre_restore_checkpoint = Some(id);
        self.set_stage("pre-restore-saved");
    }
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command.unwrap_or(Command::Save {
        message: Vec::new(),
    }) {
        Command::Save { message } => save(join_message(message)),
        Command::List => list(),
        Command::Show { checkpoint } => show(checkpoint),
        Command::Diff { checkpoint } => diff(checkpoint),
        Command::Restore { checkpoint } => restore(checkpoint),
        Command::Delete { checkpoints } => delete(checkpoints),
    }
}

fn save(message: Option<String>) -> Result<()> {
    let ctx = GitContext::discover()?;
    let store = Store::new(&ctx.git_dir);
    let _lock = RepoLock::acquire_for(&store.lock, "save")?;
    store.ensure_initialized()?;
    abort_if_incomplete_transaction(&store)?;
    let id = save_locked(&ctx, &store, Source::manual_save(), message.clone())?;
    println!("Saved checkpoint {}", short_id(&id));
    if let Some(message) = message {
        println!("Message: {message}");
    }
    Ok(())
}

fn list() -> Result<()> {
    let ctx = GitContext::discover()?;
    let store = Store::new(&ctx.git_dir);
    if !store.is_initialized() {
        return Ok(());
    }
    let _lock = RepoLock::acquire_shared(&store.lock)?;
    abort_if_incomplete_transaction(&store)?;
    let checkpoints = load_checkpoints(&store)?;
    if checkpoints.is_empty() {
        return Ok(());
    }
    println!("ID          CREATED                 SOURCE              MESSAGE");
    let ids: Vec<String> = checkpoints
        .iter()
        .map(|checkpoint| checkpoint.id.clone())
        .collect();
    for checkpoint in checkpoints {
        let prefix = unique_prefix(&checkpoint.id, &ids);
        let source = display_source(&checkpoint.manifest.source);
        let message = checkpoint.manifest.message.as_deref().unwrap_or("");
        println!(
            "{:<11} {:<23} {:<19} {}",
            prefix,
            format_time(checkpoint.manifest.created_at_utc),
            source,
            message
        );
    }
    Ok(())
}

fn show(checkpoint: Option<String>) -> Result<()> {
    let ctx = GitContext::discover()?;
    let store = Store::new(&ctx.git_dir);
    if !store.is_initialized() {
        bail!("no-checkpoint: no checkpoint exists for this worktree");
    }
    let _lock = RepoLock::acquire_shared(&store.lock)?;
    abort_if_incomplete_transaction(&store)?;
    let checkpoints = load_checkpoints(&store)?;
    let checkpoint = resolve_checkpoint(&checkpoints, checkpoint.as_deref())?;
    println!("Checkpoint  {}", checkpoint.id);
    println!(
        "Created     {}",
        format_time(checkpoint.manifest.created_at_utc)
    );
    println!(
        "Source      {}",
        display_source(&checkpoint.manifest.source)
    );
    println!(
        "Message     {}",
        checkpoint.manifest.message.as_deref().unwrap_or("")
    );
    println!("Files       {}", checkpoint.manifest.file_count());
    println!(
        "Bytes       {}",
        human_bytes(checkpoint.manifest.total_file_bytes())
    );
    println!("Format      {}", checkpoint.manifest.format_version);
    Ok(())
}

fn diff(checkpoint: Option<String>) -> Result<()> {
    let ctx = GitContext::discover()?;
    let store = Store::new(&ctx.git_dir);
    if !store.is_initialized() {
        bail!("no-checkpoint: no checkpoint exists for this worktree");
    }
    let _lock = RepoLock::acquire_shared(&store.lock)?;
    abort_if_incomplete_transaction(&store)?;
    let checkpoints = load_checkpoints(&store)?;
    let checkpoint = resolve_checkpoint(&checkpoints, checkpoint.as_deref())?;
    let current = capture(
        &ctx,
        Source {
            kind: "internal".to_owned(),
            operation: "diff".to_owned(),
            triggering_command: None,
            target_checkpoint: Some(checkpoint.id.clone()),
        },
        None,
        None,
    )?;

    let target_map = entry_map(&checkpoint.manifest);
    let current_map = entry_map(&current);
    let mut paths = BTreeSet::new();
    paths.extend(target_map.keys().cloned());
    paths.extend(current_map.keys().cloned());

    let mut changed = false;
    for path in paths {
        match (target_map.get(&path), current_map.get(&path)) {
            (None, Some(_)) => {
                changed = true;
                println!("A  {path}");
            }
            (Some(_), None) => {
                changed = true;
                println!("D  {path}");
            }
            (Some(a), Some(b)) if *a != *b => {
                changed = true;
                println!("M  {path}");
            }
            _ => {}
        }
    }
    if !changed {
        println!("No changes");
    }
    Ok(())
}

fn restore(checkpoint: Option<String>) -> Result<()> {
    let ctx = GitContext::discover()?;
    let store = Store::new(&ctx.git_dir);
    let _lock = RepoLock::acquire_for(&store.lock, "restore")?;
    store.ensure_initialized()?;
    recover_incomplete_transaction(&ctx, &store)?;
    let checkpoints = load_checkpoints(&store)?;
    let target = resolve_checkpoint(&checkpoints, checkpoint.as_deref())?.clone();

    fs::create_dir_all(&store.transactions)
        .with_context(|| format!("create {}", store.transactions.display()))?;
    let target_dir =
        TempDir::new_in(&store.transactions).context("create target materialization dir")?;
    let target_manifest = store.materialize_checkpoint(&target.id, target_dir.path())?;
    let mut journal = RestoreJournal::new(target.id.clone());
    write_journal(&store, &journal)?;

    let pre_restore_id = save_locked(&ctx, &store, Source::pre_restore(target.id.clone()), None)?;
    store.read_verified_manifest(&pre_restore_id)?;
    journal.set_pre_restore(pre_restore_id.clone());
    write_journal(&store, &journal)?;

    journal.set_stage("applying-worktree");
    write_journal(&store, &journal)?;
    let apply_result = apply_manifest(&ctx, &target_manifest, target_dir.path())
        .and_then(|_| verify_worktree(&ctx, &target_manifest));

    if let Err(err) = apply_result {
        let rollback_dir =
            TempDir::new_in(&store.transactions).context("create rollback materialization dir")?;
        let rollback_manifest =
            store.materialize_checkpoint(&pre_restore_id, rollback_dir.path())?;
        match apply_manifest(&ctx, &rollback_manifest, rollback_dir.path())
            .and_then(|_| verify_worktree(&ctx, &rollback_manifest))
        {
            Ok(()) => {
                journal.status = JournalStatus::Failed;
                journal.diagnostic =
                    Some(format!("restore failed and rollback succeeded: {err:#}"));
                journal.set_stage("rolled-back");
                write_journal(&store, &journal)?;
                bail!(
                    "restore-failed: failed to restore checkpoint {} ({err}); current workspace was restored from pre-restore checkpoint {}",
                    short_id(&target.id),
                    short_id(&pre_restore_id)
                )
            }
            Err(rollback_err) => {
                let preserved_target = target_dir.keep();
                let preserved_rollback = rollback_dir.keep();
                journal.status = JournalStatus::Failed;
                journal.diagnostic = Some(format!(
                    "restore error: {err:#}; rollback error: {rollback_err:#}; target materialization: {}; rollback materialization: {}",
                    preserved_target.display(),
                    preserved_rollback.display()
                ));
                journal.set_stage("rollback-failed");
                write_journal(&store, &journal)?;
                bail!(
                    "rollback-failed: restore failed and automatic rollback did not complete; pre-restore checkpoint {}; recovery data was preserved at {}; target materialization: {}; rollback materialization: {}; restore error: {err}; rollback error: {rollback_err}",
                    short_id(&pre_restore_id),
                    store.transactions.display(),
                    preserved_target.display(),
                    preserved_rollback.display()
                )
            }
        }
    }

    journal.status = JournalStatus::Complete;
    journal.set_stage("complete");
    write_journal(&store, &journal)?;
    clear_active_journal(&store)?;

    println!(
        "Saved current workspace as checkpoint {}",
        short_id(&pre_restore_id)
    );
    println!("Restored checkpoint {}", short_id(&target.id));
    Ok(())
}

fn delete(checkpoints_to_delete: Vec<String>) -> Result<()> {
    if checkpoints_to_delete.is_empty() {
        bail!("delete requires at least one checkpoint ID");
    }
    let ctx = GitContext::discover()?;
    let store = Store::new(&ctx.git_dir);
    if !store.is_initialized() {
        bail!("no-checkpoint: no checkpoint exists for this worktree");
    }
    let _lock = RepoLock::acquire_for(&store.lock, "delete")?;
    abort_if_incomplete_transaction(&store)?;
    let checkpoints = load_checkpoints(&store)?;
    let mut resolved = Vec::new();
    for id in &checkpoints_to_delete {
        resolved.push(resolve_checkpoint(&checkpoints, Some(id))?.id.clone());
    }
    let mut deleted = store.load_deleted()?;
    for id in &resolved {
        deleted.insert(id.clone());
    }
    store.save_deleted(&deleted)?;
    for id in resolved {
        println!("Deleted checkpoint {}", short_id(&id));
    }
    Ok(())
}

fn save_locked(
    ctx: &GitContext,
    store: &Store,
    source: Source,
    message: Option<String>,
) -> Result<String> {
    let result = (|| -> Result<String> {
        let staging_files = store.reset_staging()?;
        let manifest = capture(ctx, source.clone(), message.clone(), Some(&staging_files))?;
        store.write_staging_manifest(&manifest)?;
        let comment = match source.operation.as_str() {
            "pre-restore" => format!(
                "git-chkpt pre-restore {}",
                source.target_checkpoint.as_deref().unwrap_or("")
            ),
            _ => match message.as_deref() {
                Some(message) => format!("git-chkpt save: {message}"),
                None => "git-chkpt save".to_owned(),
            },
        };
        let staging_root = store.staging.join(crate::manifest::FILES_DIR);
        crate::snapshot::verify_materialized(&staging_root, &manifest)?;
        let id = store.commit_staging(&comment)?;
        let persisted = store.read_manifest(&id)?;
        if persisted.files != manifest.files
            || persisted.source != manifest.source
            || persisted.message != manifest.message
        {
            bail!("snapshot-failed: persisted checkpoint did not round-trip manifest metadata");
        }
        Ok(id)
    })();

    match result {
        Ok(id) => {
            cleanup_staging(store)?;
            Ok(id)
        }
        Err(err) => {
            let _ = cleanup_staging(store);
            Err(err)
        }
    }
}

fn apply_manifest(
    ctx: &GitContext,
    target: &Manifest,
    materialized_root: &std::path::Path,
) -> Result<()> {
    let current = capture(
        ctx,
        Source {
            kind: "internal".to_owned(),
            operation: "restore-scan".to_owned(),
            triggering_command: None,
            target_checkpoint: None,
        },
        None,
        None,
    )?;
    let current_map = entry_map(&current);
    let target_map = entry_map(target);

    let mut stale_paths: Vec<String> = current_map
        .keys()
        .filter(|path| !target_map.contains_key(*path))
        .cloned()
        .collect();
    stale_paths.sort_by_key(|path| std::cmp::Reverse(path.matches('/').count()));
    for path in stale_paths {
        delete_managed_path(ctx, &path)?;
    }

    for entry in &target.files {
        ensure_no_symlink_ancestors(&ctx.worktree_root, &entry.path)?;
        let dest = join_under(&ctx.worktree_root, &entry.path)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        if let Ok(meta) = fs::symlink_metadata(&dest)
            && meta.is_dir()
            && !meta.file_type().is_symlink()
        {
            fs::remove_dir(&dest).with_context(|| {
                format!(
                    "remove empty directory before restoring path {}",
                    dest.display()
                )
            })?;
        }
        match entry.kind {
            EntryKind::File => {
                let source = join_under(materialized_root, &entry.path)?;
                restore_file(&source, &dest, entry.mode.as_deref())?;
            }
            EntryKind::Symlink => {
                let target = entry
                    .target
                    .as_deref()
                    .context("corrupt-checkpoint: symlink target missing")?;
                restore_symlink(target, &dest)?;
            }
        }
    }
    Ok(())
}

fn cleanup_staging(store: &Store) -> Result<()> {
    if store.staging.exists() {
        fs::remove_dir_all(&store.staging)
            .with_context(|| format!("remove {}", store.staging.display()))?;
    }
    Ok(())
}

fn restore_file(source: &Path, dest: &Path, mode: Option<&str>) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(dest)
        && meta.is_dir()
        && !meta.file_type().is_symlink()
    {
        fs::remove_dir(dest).with_context(|| {
            format!(
                "remove empty directory before restoring path {}",
                dest.display()
            )
        })?;
    }
    let parent = dest
        .parent()
        .with_context(|| format!("restore destination has no parent: {}", dest.display()))?;
    let mut tmp = NamedTempFile::new_in(parent)
        .with_context(|| format!("create temp file in {}", parent.display()))?;
    let mut input = fs::File::open(source).with_context(|| format!("open {}", source.display()))?;
    io::copy(&mut input, tmp.as_file_mut())
        .with_context(|| format!("copy {} to temp file", source.display()))?;
    tmp.as_file_mut()
        .sync_all()
        .with_context(|| format!("sync temp file for {}", dest.display()))?;
    set_file_mode(tmp.path(), mode)?;
    tmp.persist(dest)
        .map(|_| ())
        .map_err(|err| err.error)
        .with_context(|| format!("replace {}", dest.display()))
}

fn restore_symlink(target: &str, dest: &Path) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(dest) {
        if meta.is_dir() && !meta.file_type().is_symlink() {
            fs::remove_dir(dest).with_context(|| {
                format!(
                    "remove empty directory before restoring symlink {}",
                    dest.display()
                )
            })?;
        } else {
            fs::remove_file(dest).with_context(|| {
                format!("remove path before restoring symlink {}", dest.display())
            })?;
        }
    }
    create_symlink(target, dest)
}

fn delete_managed_path(ctx: &GitContext, path: &str) -> Result<()> {
    ensure_no_symlink_ancestors(&ctx.worktree_root, path)?;
    let dest = join_under(&ctx.worktree_root, path)?;
    if let Ok(meta) = fs::symlink_metadata(&dest) {
        if meta.is_dir() && !meta.file_type().is_symlink() {
            fs::remove_dir(&dest)
                .with_context(|| format!("remove managed directory {}", dest.display()))?;
        } else {
            fs::remove_file(&dest)
                .with_context(|| format!("remove managed path {}", dest.display()))?;
        }
        remove_empty_parent_dirs(&ctx.worktree_root, path)?;
    }
    Ok(())
}

fn active_journal_path(store: &Store) -> PathBuf {
    store.transactions.join("active-restore.json")
}

fn write_journal(store: &Store, journal: &RestoreJournal) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(journal).context("serialize restore journal")?;
    atomic_write(&active_journal_path(store), &bytes)
        .context("write restore transaction journal")?;
    Ok(())
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

fn read_active_journal(store: &Store) -> Result<Option<RestoreJournal>> {
    let path = active_journal_path(store);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let journal: RestoreJournal =
        serde_json::from_slice(&bytes).context("parse restore transaction journal")?;
    if journal.format_version != 1 {
        bail!("incomplete-transaction: unsupported restore journal version");
    }
    Ok(Some(journal))
}

fn clear_active_journal(store: &Store) -> Result<()> {
    let path = active_journal_path(store);
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

fn abort_if_incomplete_transaction(store: &Store) -> Result<()> {
    if let Some(journal) = read_active_journal(store)? {
        if journal.status == JournalStatus::InProgress {
            bail!(
                "incomplete-transaction: restore to checkpoint {} stopped at stage {}; run `git chkpt restore` to attempt automatic recovery before other operations",
                short_id(&journal.target_checkpoint),
                journal.stage
            );
        }
        if journal.status == JournalStatus::Failed && is_blocking_failed_stage(&journal.stage) {
            bail!(
                "rollback-failed: previous restore did not complete safely at stage {}; recovery data was preserved at {}",
                journal.stage,
                store.transactions.display()
            );
        }
    }
    Ok(())
}

fn is_blocking_failed_stage(stage: &str) -> bool {
    matches!(stage, "rollback-failed" | "recovery-failed")
}

fn recover_incomplete_transaction(ctx: &GitContext, store: &Store) -> Result<()> {
    let Some(mut journal) = read_active_journal(store)? else {
        return Ok(());
    };
    if journal.status != JournalStatus::InProgress {
        if journal.status == JournalStatus::Failed && is_blocking_failed_stage(&journal.stage) {
            bail!(
                "rollback-failed: previous restore did not complete safely at stage {}; recovery data was preserved at {}",
                journal.stage,
                store.transactions.display()
            );
        }
        return Ok(());
    }
    let Some(pre_restore_id) = journal.pre_restore_checkpoint.clone() else {
        bail!(
            "incomplete-transaction: restore to checkpoint {} stopped before pre-restore checkpoint was recorded; no workspace files should have been modified; remove {} after inspection",
            short_id(&journal.target_checkpoint),
            active_journal_path(store).display()
        );
    };

    let rollback_dir = TempDir::new_in(&store.transactions)
        .context("create incomplete restore rollback materialization dir")?;
    let rollback_manifest = store.materialize_checkpoint(&pre_restore_id, rollback_dir.path())?;
    journal.set_stage("recovering-pre-restore");
    write_journal(store, &journal)?;
    match apply_manifest(ctx, &rollback_manifest, rollback_dir.path())
        .and_then(|_| verify_worktree(ctx, &rollback_manifest))
    {
        Ok(()) => {
            journal.status = JournalStatus::Failed;
            journal.set_stage("recovered-pre-restore");
            journal.diagnostic =
                Some("automatic recovery restored pre-restore checkpoint".to_owned());
            write_journal(store, &journal)?;
            bail!(
                "incomplete-transaction: previous restore was interrupted; current workspace was restored from pre-restore checkpoint {}. Re-run the requested command if desired.",
                short_id(&pre_restore_id)
            );
        }
        Err(err) => {
            let preserved_rollback = rollback_dir.keep();
            journal.status = JournalStatus::Failed;
            journal.set_stage("recovery-failed");
            journal.diagnostic = Some(format!(
                "automatic recovery failed: {err:#}; rollback materialization: {}",
                preserved_rollback.display()
            ));
            write_journal(store, &journal)?;
            bail!(
                "rollback-failed: previous restore was interrupted and automatic recovery did not complete; pre-restore checkpoint {}; recovery data was preserved at {}; rollback materialization: {}; recovery error: {err}",
                short_id(&pre_restore_id),
                store.transactions.display(),
                preserved_rollback.display()
            );
        }
    }
}

fn load_checkpoints(store: &Store) -> Result<Vec<Checkpoint>> {
    let deleted = store.load_deleted()?;
    let mut checkpoints = Vec::new();
    for id in store.timeline_hashes()? {
        if deleted.contains(&id) {
            continue;
        }
        match store.read_verified_manifest(&id) {
            Ok(manifest) => checkpoints.push(Checkpoint { id, manifest }),
            Err(_) => continue,
        }
    }
    checkpoints.sort_by(|a, b| {
        b.manifest
            .created_at_utc
            .cmp(&a.manifest.created_at_utc)
            .then_with(|| b.id.cmp(&a.id))
    });
    Ok(checkpoints)
}

fn resolve_checkpoint<'a>(
    checkpoints: &'a [Checkpoint],
    id: Option<&str>,
) -> Result<&'a Checkpoint> {
    if checkpoints.is_empty() {
        bail!("no-checkpoint: no checkpoint exists for this worktree");
    }
    let Some(id) = id else {
        return Ok(&checkpoints[0]);
    };
    if let Some(exact) = checkpoints.iter().find(|checkpoint| checkpoint.id == id) {
        return Ok(exact);
    }
    let matches: Vec<&Checkpoint> = checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.id.starts_with(id))
        .collect();
    match matches.as_slice() {
        [] => bail!("checkpoint-not-found: {id}"),
        [checkpoint] => Ok(*checkpoint),
        _ => bail!("ambiguous-checkpoint: {id}"),
    }
}

fn entry_map(manifest: &Manifest) -> BTreeMap<String, Entry> {
    manifest
        .files
        .iter()
        .map(|entry| (entry.path.clone(), entry.clone()))
        .collect()
}

fn display_source(source: &Source) -> String {
    let base = if source.operation == "pre-restore" {
        "pre-restore"
    } else {
        source.kind.as_str()
    };
    match source.triggering_command.as_deref() {
        Some(command) => format!("{base}:{command}"),
        None => base.to_owned(),
    }
}

fn unique_prefix(id: &str, ids: &[String]) -> String {
    for len in 8..=id.len() {
        let prefix = &id[..len];
        if ids.iter().filter(|other| other.starts_with(prefix)).count() == 1 {
            return prefix.to_owned();
        }
    }
    id.to_owned()
}

fn short_id(id: &str) -> String {
    id.chars().take(12).collect()
}

fn format_time(time: DateTime<chrono::Utc>) -> String {
    let local: DateTime<Local> = DateTime::from(time);
    local.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

pub(crate) fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
