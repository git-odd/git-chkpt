# git-chkpt

本地 Git 工作区检查点工具。

在还不打算提交（commit）、暂存（stash）或切换分支时，快速保存当前工作区的文件现场，并在需要时一键完整恢复。

---

## 为什么需要

在使用 AI 辅助编程、进行大规模重构或排查复杂问题时，我们经常需要频繁试验：

- **比 `git stash` 更自由**：不强制要求工作区干净，不与暂存区（index）冲突，支持直观的命名与多版本时间线。
- **比临时 commit 更干净**：不产生无意义的 commit，不污染 Git 历史与 commit log。
- **自带反悔机制**：每次 `restore` 前都会自动为当前现场生成检查点，随时可以撤销恢复。

---

## 快速上手

在任意 Git 仓库的工作区中直接运行：

### 1. 保存当前现场 (Save)

```bash
# 快速存档（默认命令）
git chkpt

# 附带备注信息
git chkpt save "重构解析器前"
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

## 典型工作流

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

## 工作边界与特性

- **保存范围**：
  - Git 已追踪的文件（tracked）。
  - Git 未忽略的新增文件（untracked）。
- **不受影响的内容**：
  - `.git` 内部状态（HEAD、分支、commit 历史、暂存区 index 等保持原样）。
  - `.gitignore` 忽略的文件与目录（如编译产物、本地缓存等不会被保存，也不会在恢复时被删除）。
  - Submodule 及嵌套 Git 仓库的内部文件。
- **独立隔离**：
  - 每个 Git worktree（包括 `git worktree add` 创建的 linked worktree）拥有完全独立的检查点存储，互不干扰。

---

## 安装

### 前置要求
- 系统已安装 Git

### 通过 Cargo 安装
```bash
cargo install --path .
# 或发布后：cargo install git-chkpt
```

确保 Cargo 二进制目录在系统 `PATH` 中。安装后，Git 会自动将 `git chkpt` 识别为外部命令。

---

## 进阶文档

关于架构设计、内部存储原理及详细规格，请参阅 [`docs/`](docs/) 目录：

- [技术设计与架构说明](docs/TECHNICAL_NOTES.md)
- [当前实现规格 (Implementation Spec)](docs/IMPLEMENTATION_SPEC.md)
- [产品目标草案 (Product Spec Draft)](docs/PRODUCT_SPEC_DRAFT.md)
- [产品草案覆盖矩阵 (Product Spec Coverage)](docs/PRODUCT_SPEC_COVERAGE.md)
- [项目当前状态 (Status)](docs/STATUS.md)
- [命令命名评估 (Naming Evaluation)](docs/NAMING_EVALUATION.md)
