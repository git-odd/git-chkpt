# git-chkpt

本地 Git worktree checkpoint 工具。

`git-chkpt` 用来在还不想 commit、stash 或切 branch 时，快速保存当前工作区文件现场，并在需要时完整恢复。

## 当前状态

这是一个可运行的本地原型，已经支持：

```bash
git chkpt
git chkpt save [MESSAGE]
git chkpt list    # alias: ls
git chkpt show [CHECKPOINT]
git chkpt diff [CHECKPOINT]
git chkpt restore [CHECKPOINT]
git chkpt delete <CHECKPOINT>...  # alias: rm
```

默认 `git chkpt` 等价于 `git chkpt save`。

## 适合场景

比如你正在让 AI 或 IDE 大改代码：

```bash
git chkpt save "before parser refactor"
```

然后放心改。改坏了：

```bash
git chkpt restore
```

restore 前会自动保存当前现场为一个 pre-restore checkpoint，所以恢复操作本身也可以反悔。

## 安装

需要：

- Git
- 网络访问 Fossil 官方下载站（仅首次自动准备 Fossil 时需要）
- Rust / Cargo（仅从源码构建或 `cargo install` 时需要）

`cargo install` 后可直接运行：

```bash
cargo install --path .
# 或发布后：cargo install git-chkpt
```

用户不需要单独安装 Fossil 到系统 `PATH`。运行时查找顺序：

1. `GIT_CHKPT_FOSSIL` 指定的程序路径。
2. `git-chkpt` 二进制旁边的 sidecar：同目录、`bin/`、`sidecar/`、`sidecars/`。
3. 默认启用的 `auto-fossil`：首次需要 Fossil 时按当前平台下载官方预编译包，校验 SHA3-256 后缓存到用户 cache 目录。

可用 `--no-default-features` 关闭自动下载；此时必须提供 sidecar 或 `GIT_CHKPT_FOSSIL`。开发/测试可用 `GIT_CHKPT_FOSSIL_RUNTIME_CACHE` 指定自动下载缓存目录。

构建：

```bash
cargo build --release
```

把生成的二进制放到 `PATH` 中，并保持名字为：

```text
git-chkpt
```

Git 会把：

```bash
git chkpt
```

自动转发为外部命令：

```bash
git-chkpt
```

Windows 下构建产物通常在：

```text
target/release/git-chkpt.exe
```

## 快速开始

进入任意非 bare Git worktree：

```bash
cd your-repo
```

保存当前文件现场：

```bash
git chkpt save "before risky edit"
```

输出类似：

```text
Saved checkpoint 4f18ac932e7d
Message: before risky edit
```

列出 checkpoint：

```bash
git chkpt ls
```

`git chkpt list` 也可用。

输出类似：

```text
ID          CREATED                 SOURCE              MESSAGE
4f18ac932e  2026-08-16 10:21:13.052 manual              before risky edit
```

查看最新 checkpoint：

```bash
git chkpt show
```

查看指定 checkpoint：

```bash
git chkpt show 4f18ac93
```

比较最新 checkpoint 与当前工作区：

```bash
git chkpt diff
```

输出方向是：

```text
checkpoint -> current workspace
```

示例：

```text
M  src/parser.rs
A  tests/parser_cases.rs
D  notes/old-plan.md
```

恢复最新 checkpoint：

```bash
git chkpt restore
```

恢复指定 checkpoint：

```bash
git chkpt restore 4f18ac93
```

成功输出类似：

```text
Saved current workspace as checkpoint e205be92773a
Restored checkpoint 4f18ac932e7d
```

删除 checkpoint 的公开可见性：

```bash
git chkpt rm 4f18ac93
```

`git chkpt delete` 也可用。

## 它会保存什么

保存范围是当前 Git worktree 的 managed universe：

- Git tracked 文件中当前实际存在的文件。
- Git 未忽略的 untracked 文件。

不会保存：

- `.git` 或 Git administrative directory。
- ignored 文件。
- submodule 内部内容。
- nested Git repository 内部内容。
- worktree 根目录外的内容。
- Git HEAD、branch、index、refs、hooks、config、reflog、merge/rebase 状态。

## restore 会做什么

`restore` 会把当前 managed universe 变成目标 checkpoint 的文件世界：

- 目标中存在的文件会被创建或替换。
- 目标中不存在、但当前受管理的路径会被删除。
- 文件 / 目录类型变化会恢复。
- ignored 文件不会被删除。
- submodule / nested Git repository 内部不会被父项目 restore 修改。
- Git index / HEAD / refs 不会被主动修改。

restore 前会自动执行一次内部 save，创建 pre-restore checkpoint；这个自动 checkpoint 的 `SOURCE` 会显示为 `pre-restore:restore`，表示由 `restore` 命令触发。

## 常用工作流

### 大改前存档

```bash
git chkpt save "before ai rewrite"
# run AI / refactor / experiment
git chkpt diff
```

满意后可以正常 commit：

```bash
git add .
git commit -m "rewrite parser"
```

checkpoint 不进入 Git 历史。你可以之后删除它：

```bash
git chkpt ls
git chkpt rm <id>
```

### 恢复后反悔

```bash
git chkpt restore <old-id>
```

如果发现还是恢复前的状态好，直接恢复最新的 pre-restore checkpoint：

```bash
git chkpt ls
git chkpt restore <pre-restore-id>
```

## 每个 worktree 独立

如果你使用 Git linked worktree：

```bash
git worktree add ../feature feature
```

主 worktree 和 linked worktree 的 checkpoint 是隔离的：

- 在哪个 worktree 执行 `git chkpt save`，checkpoint 就属于哪个 worktree。
- `git chkpt ls` / `git chkpt list` 只列当前 worktree 的 checkpoint。
- 不能跨 worktree 查看或恢复。

## 错误和安全性

常见错误：

- `not-a-git-worktree`：当前目录不在 Git worktree 中。
- `bare-repository`：不支持 bare repository。
- `no-checkpoint`：当前 worktree 还没有 checkpoint。
- `checkpoint-not-found`：ID 或前缀找不到。
- `ambiguous-checkpoint`：短 ID 前缀不唯一。
- `workspace-changed`：save 期间文件集合或文件内容发生变化，请重试。
- `repository-busy`：另一个 checkpoint 操作正在进行。
- `corrupt-checkpoint`：checkpoint manifest 或 payload 校验失败。

## 重要限制

- 这是本地临时 checkpoint，不是备份系统。
- checkpoint 随 `.git` 生命周期走；删除 `.git` 后不可恢复。
- 不支持云同步、remote、导入导出、跨设备恢复。
- 不支持单文件恢复。
- 不恢复 Git index 或 staged/unstaged 边界。
- `diff` 当前是 A/M/D 摘要，不是完整 unified diff。
- delete 当前是逻辑删除，不保证立刻释放磁盘空间。

## 开发验证

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```
