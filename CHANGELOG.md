# Changelog

All notable changes to `git-chkpt` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-09-03

### Fixed
- **Repository Relocation Resilience**:
  - Open Fossil checkouts using relative path `..\repository.fossil` instead of host-absolute paths, ensuring checkouts remain valid when the Git repository directory is moved or renamed.
  - Added self-healing checkout recovery: if a checkout marker points to an obsolete path or becomes invalid, `fossil_checkout` automatically catches the failure, recreates the checkout, and retries the operation transparently.
  - Updated `is_initialized` to check repository database existence rather than ephemeral checkout state.
  - Decoupled read-only operations (`list`, `show`, `diff`, `restore`) from checkout state, querying repository files directly via `-R <repo>`.
  - Added regression test `repo_relocation_preserves_checkpoint_operations` covering `list`, `show`, `save`, `diff`, and `restore` after repository directory movement.

### Performance
- **Eliminated Full-Tree Temporary Extractions**:
  - Replaced per-file extraction loops in `save_locked` and `load_checkpoints` with direct manifest inspection (`read_manifest` / `read_verified_manifest`). Checkpoint saves and listing now inspect manifest metadata directly without spawning individual `fossil cat` processes for every file in the repository.
- **Fast-Path Checkout Checking**:
  - Replaced proactive `fossil info` subprocess checks on every save with lightweight local marker checks (`0ms`), deferring recovery to error-driven self-healing.
- **Incremental Checkout Synchronization**:
  - Introduced `sync_dir_contents` to synchronize staging files into the Fossil checkout directory while preserving unchanged file timestamps and metadata.
  - Avoided repeated teardown and recreation of the checkout SQLite database (`_FOSSIL_`).
  - Added structural change tracking: skipped `fossil addremove` subprocesses when only file contents changed and no files were added or removed.
- **Subprocess Call Consolidation**:
  - Consolidated multiple `git rev-parse` queries in `GitContext::discover` into a single batch invocation.
  - Extracted newly committed checkpoint hashes directly from `fossil commit` stdout (`New_Version:`), eliminating post-commit `fossil timeline` subprocess invocations.
  - Skipped redundant `fossil settings` subprocesses during `ensure_initialized` when storage is already initialized.
