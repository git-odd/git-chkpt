<div align="center">

# ⏱️ git-chkpt

**Lightweight, local checkpoint tool for Git worktrees, powered by Fossil.**

[![Organization](https://img.shields.io/badge/Org-git--odd-blue?style=flat-square&logo=github)](https://github.com/git-odd)
[![Suite](https://img.shields.io/badge/Suite-git--odd%20Ecosystem-purple?style=flat-square&logo=git)](https://github.com/git-odd)
[![Crates.io](https://img.shields.io/crates/v/git-chkpt.svg?style=flat-square)](https://crates.io/crates/git-chkpt)
[![License](https://img.shields.io/badge/License-MIT%20%2F%20Apache--2.0-orange?style=flat-square)](LICENSE-MIT)

[English](README.md) | [简体中文](README_zh.md)

</div>

> Part of the [**`git-odd`**](https://github.com/git-odd) suite — *solving odd Git problems in odd ways.*

Quickly save snapshots of your working tree files when you are not ready to commit, stash, or switch branches—and restore them completely whenever needed. Supports both the primary command `git chkpt` and its full alias `git checkpoint`.

---

## ✨ Why git-chkpt?

When working with AI coding assistants, performing large refactorings, or debugging complex issues, you often need to experiment rapidly:

- **Powered by Fossil**: Uses the rock-solid [Fossil SCM](https://fossil-scm.org/) engine under the hood for fast deduplication and local snapshots, completely isolated from your Git history.
- **More flexible than `git stash`**: Does not require a clean working tree, never conflicts with the index/staging area, and supports clear naming with a full version timeline.
- **Cleaner than temporary commits**: Leaves no messy "wip" commits in your Git history or commit log.
- **Built-in regret safety net**: Every `restore` automatically saves the current workspace state as a `pre-restore` checkpoint, so you can easily undo any restore operation.

---

## 🚀 Installation

### Via Cargo (Recommended)

```bash
cargo install git-chkpt
```

Ensure `~/.cargo/bin` is in your system `PATH`. Cargo will automatically install both `git-chkpt` and `git-checkpoint`.

### From Git Repository

```bash
cargo install --git https://github.com/git-odd/git-chkpt.git
```

### Pre-built Binaries from GitHub Releases

Download the pre-compiled archive for your OS from the [Releases page](https://github.com/git-odd/git-chkpt/releases) and place the binaries in your system `PATH`. The archive includes the storage sidecar for zero-network, fully offline usage.

---

## 📖 Quick Start

Run directly inside any non-bare Git repository (`git chkpt` and `git checkpoint` are identical):

### 1. Save Workspace State (Save)

```bash
# Quick save (default command)
git chkpt

# Save with a descriptive message
git chkpt save "before parser refactor"

# Using the full alias
git checkpoint save "before parser refactor"
```

### 2. List Checkpoints (List)

```bash
git chkpt ls
# or: git chkpt list
```

Example output:

```text
ID          CREATED                 SOURCE    MESSAGE
4f18ac932e  2026-08-16 10:21:13.052 manual    before parser refactor
```

### 3. View Workspace Diff (Diff)

Compare the current workspace against the latest (or specified) checkpoint:

```bash
# Compare latest checkpoint against current workspace
git chkpt diff

# Compare specific checkpoint against current workspace
git chkpt diff 4f18ac93
```

Example output:

```text
M  src/parser.rs
A  tests/parser_cases.rs
D  notes/old-plan.md
```

### 4. Restore Workspace (Restore)

Restore files to the latest or a specified checkpoint:

```bash
# Restore to the latest checkpoint
git chkpt restore

# Restore to a specific checkpoint
git chkpt restore 4f18ac93
```

> **Safety Note**: Before restoring, `git-chkpt` automatically saves your current workspace as a `pre-restore` checkpoint. If you accidentally restore or change your mind, simply run `git chkpt restore` again to return to where you were.

### 5. Inspect Checkpoint Details (Show)

```bash
# Show details of the latest checkpoint
git chkpt show

# Show details of a specific checkpoint
git chkpt show 4f18ac93
```

### 6. Delete Checkpoint (Delete)

```bash
git chkpt rm 4f18ac93
# or: git chkpt delete 4f18ac93
```

---

## 🔄 Typical Workflows

### Scenario A: AI-Assisted Refactoring & Safe Experimentation

```bash
# 1. Take a snapshot before letting AI refactor code
git chkpt save "before AI refactor"

# 2. Run AI coding tools or write experimental changes
# ... review code, run test suite ...

# 3. Check summary of changes
git chkpt diff

# 4. If unsatisfied, roll back in one step
git chkpt restore
```

### Scenario B: Undoing a Restore

```bash
# After restoring an older state, you realize you still need recent edits:
git chkpt ls
# Find the automatic pre-restore checkpoint and restore it:
git chkpt restore <pre-restore-id>
```

---

## 🛡️ Boundaries & Characteristics

- **Included in Checkpoints**:
  - All Git tracked files.
  - Non-ignored untracked files.
- **Untouched & Preserved**:
  - `.git` internal state (HEAD, branches, commit history, index / staging area remain intact).
  - `.gitignore` ignored files and directories (build artifacts, caches, etc. are not saved and never removed on restore).
  - Submodules and nested Git repositories.
- **Worktree Isolation**:
  - Each Git worktree (including linked worktrees created via `git worktree add`) has completely isolated checkpoint storage.

## 📚 Deep Dive & Documentation

For architecture design, storage mechanisms, and formal specifications, please refer to the [`docs/`](docs/) directory:

- [Technical Notes & Architecture](docs/TECHNICAL_NOTES.md)
- [Current Implementation Specification](docs/IMPLEMENTATION_SPEC.md)
- [Product Specification Draft](docs/PRODUCT_SPEC_DRAFT.md)
- [Product Spec Coverage Matrix](docs/PRODUCT_SPEC_COVERAGE.md)
- [Project Status](docs/STATUS.md)
- [Naming Evaluation](docs/NAMING_EVALUATION.md)

---

## 📄 License

Dual-licensed under either of:
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

