<div align="center">

# ⏱️ git-chkpt

**基于 Fossil 引擎、轻量免配置的 Git 工作区快速存档与回滚工具。**

[![Organization](https://img.shields.io/badge/Org-git--odd-blue?style=flat-square&logo=github)](https://github.com/git-odd)
[![Suite](https://img.shields.io/badge/Suite-git--odd%20Ecosystem-purple?style=flat-square&logo=git)](https://github.com/git-odd)
[![Crates.io](https://img.shields.io/crates/v/git-chkpt.svg?style=flat-square)](https://crates.io/crates/git-chkpt)
[![License](https://img.shields.io/badge/License-MIT%20%2F%20Apache--2.0-orange?style=flat-square)](LICENSE-MIT)

[English](README.md) | [简体中文](README_zh.md)

</div>

> 隶属于 [**`git-odd`**](https://github.com/git-odd) 工具家族 — *用奇怪的方式解决 Git 奇怪的问题。*

在开发未完成、不便执行 `git commit`、`git stash` 或切换分支时，随时为工作区文件保存快照（Checkpoint），并在需要时完整还原。支持主命令 `git chkpt` 以及完整别名 `git checkpoint`。

---

## ✨ 为什么选择 git-chkpt？

在进行 AI 辅助编程、大规模重构或排查疑难 Bug 时，经常需要快速试错与频繁存档：

- **基于 Fossil 引擎**：底层调用极其可靠的 [Fossil SCM](https://fossil-scm.org/) 存储引擎，实现极速去重与独立快照，与 Git 历史完全隔离。
- **比 `git stash` 更自由**：不要求工作区干净，不与暂存区（Index）冲突，支持清晰的命名和完整的版本时间线。
- **比临时提交更纯净**：不会在 Git 提交历史中留下任何杂乱的 "wip"、"temp" 提交。
- **自带反悔机制（安全网）**：每次执行 `restore` 恢复时，都会自动将当前工作区保存为一个 `pre-restore` 检查点，任何恢复操作都可一键反悔。

---

## 🚀 安装指南

### 方式一：通过 Cargo 安装（推荐）

```bash
cargo install git-chkpt
```

确保 Cargo 二进制目录（`~/.cargo/bin`）在系统 `PATH` 中。安装时会自动包含 `git-chkpt` 和 `git-checkpoint` 两个命令。

### 方式二：从 Git 仓库安装

```bash
cargo install --git https://github.com/git-odd/git-chkpt.git
```

### 方式三：通过 GitHub Release 下载预编译包（零网络开箱即用）

从 [Releases 页面](https://github.com/git-odd/git-chkpt/releases) 下载对应平台的预编译压缩包，解压后将二进制放入系统 `PATH` 目录即可。压缩包内置了存储引擎 sidecar，完全离线可用。

---

## 📖 快速上手

在任意非 bare Git 仓库根目录或子目录下直接运行（`git chkpt` 与 `git checkpoint` 等效）：

### 1. 保存当前工作区 (Save)

```bash
# 快速保存（默认命令）
git chkpt

# 带描述信息保存
git chkpt save "重构解析器前"

# 使用完整别名
git checkpoint save "重构解析器前"
```

### 2. 查看检查点列表 (List)

```bash
git chkpt ls
# 或: git chkpt list
```

输出示例：

```text
ID          CREATED                 SOURCE    MESSAGE
4f18ac932e  2026-08-16 10:21:13.052 manual    重构解析器前
```

### 3. 查看改动差异 (Diff)

查看当前工作区与最新（或指定）检查点的文件变动摘要：

```bash
# 比较最新检查点与当前工作区
git chkpt diff

# 比较指定检查点与当前工作区
git chkpt diff 4f18ac93
```

输出示例：

```text
M  src/parser.rs
A  tests/parser_cases.rs
D  notes/old-plan.md
```

### 4. 恢复现场 (Restore)

恢复到最新或指定的检查点：

```bash
# 恢复到最新检查点
git chkpt restore

# 恢复到指定检查点
git chkpt restore 4f18ac93
```

> **安全提示**：执行 `restore` 前，工具会自动将当前工作区保存为一个 `pre-restore` 检查点。如果不小心恢复错误，直接再次 `git chkpt restore` 即可回到恢复前的状态。

### 5. 查看检查点详情 (Show)

```bash
# 查看最新检查点详情
git chkpt show

# 查看指定检查点详情
git chkpt show 4f18ac93
```

### 6. 清理检查点 (Delete)

```bash
git chkpt rm 4f18ac93
# 或: git chkpt delete 4f18ac93
```

---

## 🔄 典型工作流

### 场景 A：AI 辅助编程与重构试错

```bash
# 1. 在 AI 大幅修改代码前快速存档
git chkpt save "AI 重构前"

# 2. 运行 AI 工具或编写实验性代码
# ... 编写代码、运行测试 ...

# 3. 检查代码改动概况
git chkpt diff

# 4. 如果实验不满意，一键撤销所有改动
git chkpt restore
```

### 场景 B：恢复后反悔

```bash
# 恢复了某个旧版本后，发现刚才未提交的修改依然需要
git chkpt ls
# 找到自动生成的 pre-restore 检查点并恢复
git chkpt restore <pre-restore-id>
```

---

## 🛡️ 工作边界与特性

- **保存范围**：
  - Git 已追踪的文件（tracked）。
  - Git 未忽略的新增文件（untracked）。
- **不受影响的内容**：
  - `.git` 内部状态（HEAD、分支、commit 历史、暂存区 index 等保持原样）。
  - `.gitignore` 忽略的文件与目录（如编译产物、本地缓存等不会被保存，也不会在恢复时被删除）。
  - Submodule 及嵌套 Git 仓库的内部文件。
- **独立隔离**：
  - 每个 Git worktree（包括 `git worktree add` 创建的 linked worktree）拥有完全独立的检查点存储，互不干扰。

## 📚 进阶文档

关于架构设计、内部存储原理及详细规格，请参阅 [`docs/`](docs/) 目录：

- [技术设计与架构说明](docs/TECHNICAL_NOTES.md)
- [当前实现规格 (Implementation Spec)](docs/IMPLEMENTATION_SPEC.md)
- [产品目标草案 (Product Spec Draft)](docs/PRODUCT_SPEC_DRAFT.md)
- [产品草案覆盖矩阵 (Product Spec Coverage)](docs/PRODUCT_SPEC_COVERAGE.md)
- [项目当前状态 (Status)](docs/STATUS.md)
- [命令命名评估 (Naming Evaluation)](docs/NAMING_EVALUATION.md)

---

## 📄 开源许可证

本项目采用双许可证授权，您可以按需选择以下任一许可使用：
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) 或 <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT License ([LICENSE-MIT](LICENSE-MIT) 或 <http://opensource.org/licenses/MIT>)

