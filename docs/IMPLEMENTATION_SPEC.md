# git-chkpt Implementation Specification

状态：Implementation SPEC v0.1
日期：2026-08-16

本文描述当前代码实际实现的行为。原始产品草案见 `docs/PRODUCT_SPEC_DRAFT.md`。

## 1. 命令面

公开命令为：

```bash
git chkpt
git chkpt save [MESSAGE]
git chkpt list
git chkpt show [CHECKPOINT]
git chkpt diff [CHECKPOINT]
git chkpt restore [CHECKPOINT]
git chkpt delete <CHECKPOINT>...
```

实现二进制名为 `git-chkpt`，作为 Git external command 使用。

`git chkpt` MUST 等价于 `git chkpt save`。

## 2. 运行环境

实现要求：

- 当前目录 MUST 位于非 bare Git worktree 中。
- 系统 PATH 中 MUST 能执行 `git`。
- 系统 PATH 中 MUST 能执行 `fossil`。
- Fossil MUST 只作为本地存储后端使用。

如果当前 repository 是 bare repository，命令 MUST 失败并返回 `bare-repository` 语义错误。

如果当前目录不是 Git worktree，命令 MUST 失败并返回 `not-a-git-worktree` 语义错误。

## 3. Worktree 解析

实现 MUST 使用 Git 命令解析当前 worktree：

```bash
git rev-parse --show-toplevel
git rev-parse --git-dir
```

实现 MUST NOT 假定 `.git` 一定是 worktree 根目录下的目录。

解析结果经过词法绝对化，避免 Windows verbatim path 对 Fossil CLI 造成误解析。

## 4. 存储布局

当前 worktree 的 checkpoint 私有目录为：

```text
<git-dir>/git-chkpt/
```

布局为：

```text
git-chkpt/
├── repository.fossil
├── checkout/
├── staging/
├── transactions/
│   └── active-restore.json
├── lock
├── version
└── deleted.json
```

字段含义：

- `repository.fossil`：唯一 Fossil repository，保存 checkpoint check-in。
- `checkout/`：私有 Fossil checkout；用户 worktree 永远不是 Fossil checkout。
- `staging/`：save / restore 临时物化目录。
- `transactions/`：restore journal 与失败诊断材料。
- `lock`：当前 worktree checkpoint 操作锁。
- `version`：私有目录布局版本。
- `deleted.json`：逻辑删除 checkpoint ID 集合。

## 5. Fossil 使用

实现使用 Fossil CLI。

初始化 MUST：

- `fossil init` 创建本地 repository。
- `fossil open --empty --nested --nosync --force` 创建私有 checkout。
- `fossil settings autosync off` 显式关闭 autosync。

保存 checkpoint MUST：

- 将 manifest 与文件 payload 写入 staging。
- 同步 staging 到私有 Fossil checkout。
- 执行 `fossil addremove --dotfiles`。
- 执行 `fossil commit --private --allow-empty --nosync`。
- 使用 Fossil check-in full hash 作为 checkpoint 完整 ID。

实现 MUST NOT：

- 配置 Fossil remote。
- 执行 Fossil push / pull / sync / clone。
- 依赖用户理解 Fossil 工作流。

## 6. Managed Universe

保存和恢复的文件集合称为 managed universe。

当前实现枚举：

```bash
git ls-files --stage -z
git ls-files -o --exclude-standard -z
```

包含：

- 当前实际存在的 tracked 普通文件或 symlink。
- Git 未忽略的 untracked 普通文件或 symlink。

不包含：

- ignored 路径。
- `.git` 或 Git administrative data。
- submodule 内部内容。
- nested Git repository 内部内容。
- worktree 根目录外内容。
- 已从工作区删除但仍 tracked 的文件内容。
- 空目录。

Tracked mode `160000` 被视为 submodule boundary。

Untracked 候选路径如果自身或祖先包含 `.git`，被视为 nested Git repository boundary 并排除。

## 7. Manifest 格式

每个 checkpoint 必含 `manifest.json`：

```json
{
  "format_version": 1,
  "created_at_utc": "2026-08-16T00:00:00.000000Z",
  "source": {
    "kind": "manual",
    "operation": "save"
  },
  "message": "optional message",
  "files": [
    {
      "path": "src/main.rs",
      "type": "file",
      "mode": "100644",
      "size": 123,
      "sha256": "...64 hex chars..."
    }
  ]
}
```

Pre-restore checkpoint 使用：

```json
{
  "source": {
    "kind": "automatic",
    "operation": "pre-restore",
    "target_checkpoint": "..."
  }
}
```

Manifest validation MUST enforce：

- `format_version == 1`。
- path 非空。
- path 不是绝对路径。
- path 不包含 NUL。
- path 不含 `.` / `..` / 空组件。
- path 不能是 `.git` 或 `.git/...`。
- paths 按字典序排序。
- 不允许重复 path。
- 不允许 path 与其 ancestor 同时作为 entry 出现。
- 不允许大小写折叠后冲突。
- file entry 必须包含 `mode`、`size`、`sha256`。
- file `mode` 只能是 `100644` 或 `100755`。
- file `sha256` 必须是 64 位 hex。
- symlink entry 必须包含 UTF-8 target，且 target 不含 NUL。

## 8. Checkpoint Payload

Fossil check-in payload 目前固定为：

```text
manifest.json
files/<logical path>...
```

普通文件内容存储在 `files/<path>`。

Symlink 只记录在 manifest 中，不在 `files/` 下保存 payload。

读取或恢复 checkpoint 前 MUST 验证：

- manifest 可读取并可解析。
- manifest validation 通过。
- Fossil check-in 中实际路径集合等于 manifest 推导出的集合。
- 普通文件可 materialize。
- materialized 文件大小和 SHA-256 与 manifest 一致。
- materialized 目录中没有 manifest 未声明的普通文件。

校验失败的 checkpoint MUST 不作为正常 checkpoint 展示或恢复。

## 9. Save 行为

`save` MUST：

1. 解析当前 Git worktree。
2. 获取当前 worktree 的独占 lock。
3. 初始化或打开 Fossil store。
4. 检查未完成 restore transaction。
5. 重建 staging。
6. 枚举 managed universe。
7. 复制文件到 staging。
8. 捕获 symlink target。
9. 计算普通文件 size / SHA-256。
10. 生成 manifest。
11. 验证 staging 与 manifest 一致。
12. 提交 Fossil check-in。
13. 重新 materialize 并校验 checkpoint。
14. 清理 staging。
15. 输出 checkpoint 短 ID。

Save MUST NOT 修改用户 worktree 文件。

Save MUST NOT 主动修改 Git index、HEAD、refs、config、hooks 或 Git objects。

并发修改检测：

- 复制普通文件前后比较 size 与 mtime。
- 复制结束后重新枚举 managed universe。
- 捕获结束后重新验证已观察 path 的类型、size、mtime 或 symlink target。
- 检测到不一致 MUST 返回 `workspace-changed`。

Save 失败后 MUST 尝试清理 staging。

## 10. List 行为

`list`：

- 未初始化时成功返回空输出。
- 已初始化时获取 shared lock。
- 如果存在未完成 restore journal，则拒绝普通读取。
- 只列当前 worktree Fossil repository 中未逻辑删除且校验通过的 checkpoint。
- 按 manifest `created_at_utc` 倒序排列。
- 展示本机时区时间，包含 UTC offset。
- ID 展示为当前列表内唯一短前缀，最短 8 字符。

输出列：

```text
ID          CREATED                         SOURCE        MESSAGE
```

`SOURCE` 对 manual save 显示 `manual`，对 pre-restore 显示 `pre-restore`。

## 11. Show 行为

`show [CHECKPOINT]`：

- 省略 checkpoint 时选择最新有效 checkpoint。
- 接受完整 hash 或唯一短前缀。
- 未初始化或无 checkpoint 时返回 `no-checkpoint`。
- 展示完整 checkpoint ID、创建时间、source、message、文件数、字节数、format version。

## 12. Diff 行为

`diff [CHECKPOINT]`：

- 省略 checkpoint 时选择最新有效 checkpoint。
- 比较方向是 `checkpoint -> current workspace`。
- 输出 path-level 摘要：
  - `A` 当前新增。
  - `D` 当前删除。
  - `M` 内容、类型或 metadata 不同。
- 当前实现不输出 unified diff。
- `diff` MUST NOT 修改 worktree 或 Fossil checkout payload。

## 13. Restore 行为

`restore [CHECKPOINT]` MUST：

1. 解析当前 Git worktree。
2. 获取当前 worktree 独占 lock。
3. 初始化或打开 Fossil store。
4. 如果发现 incomplete restore journal，尝试恢复到 pre-restore checkpoint。
5. 解析目标 checkpoint。
6. materialize 并验证目标 checkpoint。
7. 创建 active restore journal。
8. 执行内部 pre-restore save。
9. 验证 pre-restore checkpoint 可 materialize。
10. 更新 journal 到 `pre-restore-saved`。
11. 扫描当前 managed universe。
12. 删除目标 manifest 中不存在的当前 managed paths。
13. 创建必要目录。
14. 使用同目录随机临时文件替换普通文件。
15. 恢复 symlink。
16. 验证最终 worktree manifest 与目标 manifest 一致。
17. 标记 journal complete 并清除 active journal。
18. 输出 pre-restore 和目标 checkpoint ID。

Restore MUST NOT 主动修改 Git index、HEAD、refs、config、hooks 或 Git objects。

Restore MUST NOT 删除 ignored 文件。

Restore 对普通文件使用 `NamedTempFile` 同目录写入，并 `persist` 到目标路径，避免固定临时文件名覆盖用户文件。

Restore 会为文件/目录类型变化移除空目录或替换普通文件。

## 14. Restore Failure / Rollback

如果 restore 修改工作区后失败，工具 MUST：

- 使用 pre-restore checkpoint 尝试 rollback。
- rollback 成功时报告 restore 失败但工作区已回到 pre-restore 状态。
- rollback 失败时保留 active journal 和诊断信息。
- rollback 失败时保留目标 materialization 与 rollback materialization 目录。
- 不删除 pre-restore checkpoint。

下次执行 `restore` 时，如果发现 `active-restore.json` 是 `in-progress` 且包含 pre-restore checkpoint，则尝试自动恢复到 pre-restore checkpoint。

自动恢复成功后，命令返回错误，提示用户重新运行原命令。

自动恢复失败后，命令返回 `rollback-failed` 并保留诊断目录。

## 15. Delete 行为

`delete <CHECKPOINT>...`：

- 必须至少提供一个 ID。
- 获取当前 worktree 独占 lock。
- 先解析全部 ID。
- 如果任何 ID 不存在或不唯一，MUST 不执行部分删除。
- 删除是逻辑删除：把完整 checkpoint ID 写入 `deleted.json`。
- `deleted.json` 使用同目录临时文件原子写入。
- delete 后，公开命令 `list/show/diff/restore/delete` 不再能通过该 ID 正常访问该 checkpoint。
- Fossil check-in 仍可能物理存在；不保证回收磁盘空间。

## 16. Locking

`save`、`restore`、`delete` 使用独占文件锁。

`list`、`show`、`diff` 使用 shared 文件锁。

Lock 文件保存元信息：

```text
pid=<pid>
operation=<operation>
started_at_utc=<timestamp>
```

busy 时错误中会尽力展示 holder 信息。

## 17. Atomic Writes

以下私有状态写入使用 `NamedTempFile` + `sync_all` + `persist`：

- `transactions/active-restore.json`
- `deleted.json`

普通文件 restore 也使用同目录 `NamedTempFile` + `sync_all` + `persist`。

## 18. 当前明确不支持

- 单文件 restore。
- 保存或恢复空目录。
- 保存 Git index / staged 状态。
- 保存 Git HEAD / branch / refs。
- 保存 merge / rebase / cherry-pick sequencer 状态。
- 跨 worktree 查看或恢复 checkpoint。
- checkpoint import / export。
- remote / cloud sync。
- 自动定时 checkpoint。
- GUI / IDE plugin。
- delete 的物理空间回收。
- 完整 unified diff。

## 19. 验收测试

当前测试文件：`tests/smoke.rs`。

覆盖内容包括：

- save / diff / restore 基本流程。
- ignored 文件保留。
- pre-restore 可逆恢复。
- delete 公开不可见。
- save 不改变 Git status / index。
- restore 不改变 Git index 字节。
- linked worktree checkpoint 隔离。
- 文件 / 目录类型切换。
- 真实 submodule 隔离。
- nested Git repository 隔离。
- 只读命令不初始化存储。
- 默认命令保存 checkpoint。
- `.git` 生命周期。

验证命令：

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```
