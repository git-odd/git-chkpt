# git-chkpt 当前状态

日期：2026-08-16

## 一句话状态

`git-chkpt` 现在已经是一个可运行的本地 Git checkpoint 原型：能在当前 Git worktree 里保存、列出、查看、比较、恢复、逻辑删除 checkpoint，并且 restore 前会自动保存当前现场，确保正常情况下可反悔。

它不是 Git commit / branch / stash 的替代品，而是“正式提交前的本地安全存档”。

## 已实现能力

- `git chkpt` 等价于 `git chkpt save`。
- `git chkpt save [MESSAGE]` 保存当前 managed universe 的完整文件快照。
- `git chkpt list` / `git chkpt ls` 列出当前 worktree 的有效 checkpoint。
- `git chkpt show [CHECKPOINT]` 显示 checkpoint 元数据和摘要。
- `git chkpt diff [CHECKPOINT]` 显示 checkpoint 到当前工作区的 A/M/D 摘要。
- `git chkpt restore [CHECKPOINT]` 恢复到目标 checkpoint；省略 ID 时恢复最新 checkpoint。
- `git chkpt delete <CHECKPOINT>...` / `git chkpt rm <CHECKPOINT>...` 从公开命令中隐藏 checkpoint。
- 每个 Git worktree 使用独立 checkpoint 存储。
- linked worktree 与主 worktree 的 checkpoint 物理隔离。
- restore 前自动创建 pre-restore checkpoint，并在 `SOURCE` 中标出触发命令（例如 `pre-restore:restore`）。
- restore 失败后会尝试自动 rollback 到 pre-restore checkpoint。
- save 不修改用户文件、Git index、HEAD、refs。
- restore 不主动修改 Git index、HEAD、refs。
- ignored 文件不被保存，也不被 restore 删除。
- submodule / nested Git repository 内部内容不被父项目 checkpoint 递归管理。
- 自动获取或使用随包发布的 Fossil CLI；用户不需要单独安装系统级 `fossil`。
- Fossil autosync 显式关闭；不配置 remote；不执行网络同步。

## 存储位置

checkpoint 数据保存到当前 worktree 专属 Git administrative directory 下：

```text
<worktree-git-dir>/git-chkpt/
├── repository.fossil
├── checkout/
├── staging/
├── transactions/
├── lock
├── version
└── deleted.json
```

主 worktree 通常是：

```text
.git/git-chkpt/
```

linked worktree 通常是：

```text
.git/worktrees/<name>/git-chkpt/
```

## 当前测试覆盖

当前集成测试覆盖：

- 保存 / diff / 恢复基本流程。
- ignored 文件保留。
- pre-restore 可逆恢复与 `SOURCE` 触发命令展示。
- list/delete 别名 `ls` / `rm`。
- save 不改变 Git status / index。
- restore 不改变 Git index 字节。
- linked worktree checkpoint 隔离。
- 文件与目录类型切换恢复。
- 真实 submodule 不递归进入。
- nested Git repository 不递归进入。
- 只读命令在未初始化时不创建 checkpoint 存储。
- `.git` 删除并重新 `git init` 后不继承旧 checkpoint。
- list 时间展示不带 UTC offset。
- 无子命令默认保存。

最近验证命令：

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

两者通过。

## 当前重要限制

- `diff` 目前只显示 `A/M/D` 摘要，不输出 unified diff。
- delete 是逻辑删除：通过 `deleted.json` 隐藏 checkpoint，不物理回收 Fossil 历史空间。
- restore crash recovery 有基础 journal 和自动恢复逻辑，但还不是完整状态机；尚未做系统化 crash injection 测试。
- Windows symlink 支持受系统权限影响；当前恢复 symlink 使用 file symlink 语义。
- 空目录不保存。
- 不支持单文件恢复。
- 不恢复 Git HEAD、branch、index、staged/unstaged 边界或 merge/rebase/cherry-pick 状态。
- checkpoint 依赖当前 `.git` 生命周期；删除 `.git` 或重新初始化后，旧 checkpoint 不会被搜索或重新关联。

## 适合怎么用

适合：

- 重构前快速存档。
- AI / IDE 大改代码前保存当前工作区。
- 频繁试错时保存多个临时状态。
- 不想 commit、不想 stash、不想切 branch，但想保留当前文件现场。

不适合：

- 长期备份。
- 跨机器同步。
- 团队共享。
- 替代正式 Git commit。
- 保存 Git 内部状态。
