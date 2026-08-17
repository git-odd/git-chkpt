# Technical Notes: Why git-chkpt Works This Way

本文从技术角度说明当前实现里最值得关注的几个设计选择。

## 1. Git 负责枚举，Fossil 负责存储

`git-chkpt` 不自己猜哪些文件应该进入 checkpoint。

它使用 Git 的只读命令确定 managed universe：

```bash
git ls-files --stage -z
git ls-files -o --exclude-standard -z
```

这样做的好处：

- ignored 规则交给 Git 解释。
- tracked / untracked 边界与 Git 的视角一致。
- submodule gitlink 能通过 mode `160000` 识别。
- 不需要自己实现 `.gitignore` parser。

Fossil 则被用作本地内容数据库：

- 每个 checkpoint 是一个 Fossil check-in。
- Fossil check-in hash 是 checkpoint 内部完整 ID。
- Fossil 负责内容去重和历史存储。

关键点是：用户 worktree 永远不是 Fossil checkout。Fossil checkout 放在私有目录：

```text
<git-dir>/git-chkpt/checkout/
```

这样 Fossil 操作不会污染用户工作区。

## 2. 为什么存储放在 worktree 专属 git-dir 下

Git linked worktree 共享 object store，但每个 worktree 有自己的 Git administrative directory。

如果 checkpoint 存在 common `.git` 下，会导致：

- 主 worktree 和 linked worktree 互相看见 checkpoint。
- 一个 worktree 可能恢复另一个 worktree 的文件世界。
- checkpoint 生命周期不再跟随当前 worktree。

所以实现使用：

```bash
git rev-parse --git-dir
```

得到当前 worktree 专属 git-dir，并把存储放到：

```text
<worktree-git-dir>/git-chkpt/
```

主 worktree 示例：

```text
project/.git/git-chkpt/
```

linked worktree 示例：

```text
main/.git/worktrees/feature/git-chkpt/
```

这让 checkpoint 自然满足 worktree 隔离。

## 3. Manifest 是契约，Fossil 是载体

每个 checkpoint 中都包含一个自描述 manifest：

```text
manifest.json
files/...
```

Manifest 记录：

- format version
- UTC 创建时间
- source
- message
- path 列表
- 文件类型
- mode
- size
- SHA-256
- symlink target

恢复时不信任 Fossil check-in 本身“看起来有文件”就直接写回，而是：

1. 读 manifest。
2. 校验 manifest 格式。
3. 校验 Fossil check-in 的 payload 路径集合与 manifest 一致。
4. materialize 文件。
5. 校验 materialized 文件 size / SHA-256。
6. 再进入 restore。

这能防止外部手动修改 Fossil repository 后产生危险恢复。

## 4. Save 的一致性防线

保存时无法获得操作系统级瞬时快照，所以当前实现用多道检测避免明显不一致：

- 保存开始先枚举 managed universe。
- 每个普通文件复制前读取 metadata。
- 复制后重新读取 metadata，比较 size / mtime。
- 对 staging 中的复制内容计算 SHA-256。
- 保存结束重新枚举 managed universe。
- 再检查已观察 path 的类型、size、mtime 或 symlink target。

如果发现变化，返回：

```text
workspace-changed
```

并清理 staging。

这不是完美快照算法，但足以拒绝明显跨时刻混合的 checkpoint。

## 5. Restore 为什么先保存 pre-restore

restore 是破坏性文件操作：会替换文件，也会删除目标 checkpoint 中不存在的 managed paths。

为了让 restore 可逆，真正改用户文件之前必须先保存当前 managed universe：

```text
current workspace -> pre-restore checkpoint -> apply target checkpoint
```

成功 restore 输出：

```text
Saved current workspace as checkpoint e205be92773a
Restored checkpoint 4f18ac932e7d
```

如果用户想反悔，可以从 `list` 里找到 pre-restore checkpoint，再 restore 它。

## 6. Restore Journal 的作用

Restore 过程中会写：

```text
<git-dir>/git-chkpt/transactions/active-restore.json
```

它记录：

- target checkpoint
- pre-restore checkpoint
- 当前 stage
- status
- diagnostic
- timestamps

如果 restore 被中断，下次执行 `git chkpt restore` 会先检查这个 journal。

如果 journal 表明已经有 pre-restore checkpoint，就尝试自动恢复到 pre-restore 状态。

如果恢复失败，会保留 journal 和物化目录，避免假装安全结束。

## 7. 为什么 ignored 文件不能简单递归删除

Restore 的目标是让 managed universe 与 checkpoint 一致，但 ignored 文件不属于 managed universe。

假设当前工作区：

```text
build/old.rs        managed
build/cache.bin     ignored
```

目标 checkpoint 没有 `build/old.rs`。

restore 必须删除：

```text
build/old.rs
```

但必须保留：

```text
build/cache.bin
```

所以实现不能 `remove_dir_all("build")`。当前策略是只删除枚举出的 managed path，然后向上清理空父目录；如果目录里还有 ignored 内容，目录不会被删。

## 8. 同目录随机临时文件替换

普通文件恢复使用：

```text
NamedTempFile::new_in(parent)
write + sync_all + persist(dest)
```

原因：

- 临时文件与目标文件同目录，避免跨文件系统 rename 问题。
- 随机文件名避免覆盖用户可能已有的固定临时名。
- `persist` 比先删除再写入更接近原子替换语义。
- 写入失败时原目标文件尽可能仍在。

早期实现使用固定 pid 后缀临时名，存在覆盖用户文件风险；现在已改为随机临时文件。

## 9. delete 为什么是逻辑删除

Fossil check-in 是历史对象，物理删除并不是常规工作流。

当前产品语义是：

```text
delete 后，该 checkpoint 不再能通过 git-chkpt 公开命令访问。
```

实现方式是维护：

```text
deleted.json
```

`list/show/diff/restore/delete` 只处理未逻辑删除的 checkpoint。

这避免了重写 Fossil repository 或破坏剩余历史。

## 10. 当前仍值得后续加强的点

### Crash recovery 状态机

当前 journal 已能处理基础 incomplete transaction，但还不是完整精细状态机。后续可以补：

- 每个 restore plan step 的持久记录。
- crash injection 测试。
- journal completion / failure 的明确清理策略。

### Unified diff

当前 `diff` 是 path summary：

```text
A  path
M  path
D  path
```

后续可以对文本文件输出 unified diff，对二进制输出 binary changed。

### Corrupt checkpoint 诊断

当前损坏 checkpoint 不作为正常 checkpoint 展示或恢复。后续可增加诊断命令，让用户看见并处理损坏项。

### Windows symlink / reparse point

Windows symlink 受权限和类型影响。当前恢复使用 file symlink 语义，后续可记录 symlink 目标类型或增加平台降级策略。

## 11. 核心工程判断

这个项目最关键的工程判断是：

> Git 决定“哪些路径属于当前工作区文件世界”，Fossil 保存“这些路径的内容历史”，manifest 作为二者之间的可验证契约。

这个分工让实现保持克制：

- 不重新实现 Git。
- 不暴露 Fossil 工作流给用户。
- 不引入抽象后端。
- 不试图恢复 Git 内部状态。
- 把 checkpoint 明确定义为“文件世界快照”。
