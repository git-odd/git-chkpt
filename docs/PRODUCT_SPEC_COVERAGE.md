# Product Spec Draft Coverage

日期：2026-08-16

本文对照 `PRODUCT_SPEC_DRAFT.md`，说明当前代码对产品草案的实现状态。

状态含义：

- **完整实现**：当前代码实现了该项核心语义，并已有直接或间接测试覆盖。
- **基本实现**：当前代码实现了主要行为，但仍有边界、测试或语义细节未完全覆盖。
- **部分实现**：已有实现雏形，但距离草案的完整要求仍有明显缺口。
- **未实现**：草案提出了该能力或验收项，但当前代码没有实现。
- **开放问题**：草案本身尚未决定，当前实现只选择了一个临时策略或尚未覆盖。
- **非目标**：草案明确不要求实现；当前代码也不应实现。

## 总览

| 模块 | 当前状态 | 说明 |
|---|---:|---|
| 公共命令面 | 完整实现 | `save/list/show/diff/restore/delete` 与默认 `git chkpt` 均可用。 |
| 每 worktree 独立存储 | 完整实现 | 使用当前 worktree 专属 Git admin dir，linked worktree 有测试覆盖。 |
| Fossil 本地后端 | 完整实现 | 使用 Fossil repository + 私有 checkout，关闭 autosync。 |
| Save 基础契约 | 基本实现 | 无工作区副作用、manifest、hash、staging、round-trip 校验已实现。 |
| Restore 基础契约 | 基本实现 | pre-restore、完整文件恢复、ignored 保留、rollback 已实现。 |
| Delete 契约 | 基本实现 | 当前实现为逻辑删除，不做物理删除。符合草案初步产品语义。 |
| Integrity check | 基本实现 | manifest 和 Fossil payload 校验已做；诊断可见性仍不足。 |
| Crash recovery | 部分实现 | 有 active journal 和自动恢复雏形；尚未完整状态机和 crash injection。 |
| Diff | 部分实现 | 只有 A/M/D 摘要；未实现 unified diff / binary changed。 |
| Git 状态独立 | 部分实现 | 不主动修改 Git state；测试覆盖 index，但未覆盖 merge/rebase/cherry-pick 等。 |
| 文件类型与平台特殊项 | 部分实现 | 普通文件/symlink 支持；空目录、Windows reparse point、hard link 等未完整定义。 |
| 长期/跨设备/共享能力 | 非目标 | 当前代码没有实现，符合草案非目标。 |

## 完整实现

### 1. 公共命令集合

草案要求：

```bash
git chkpt save [MESSAGE]
git chkpt list
git chkpt show [CHECKPOINT]
git chkpt diff [CHECKPOINT]
git chkpt restore [CHECKPOINT]
git chkpt delete <CHECKPOINT>...
git chkpt
```

当前状态：**完整实现**。

实现情况：

- `git chkpt` 等价于 `git chkpt save`。
- `save` 支持可选 message。
- `show/diff/restore` 省略 ID 时默认使用最新有效 checkpoint。
- `show/diff/restore/delete` 支持完整 hash 或唯一前缀。
- `delete` 支持一次删除多个 checkpoint。

测试覆盖：

- `default_command_saves_checkpoint`
- `save_diff_restore_preserves_ignored_files`
- `restore_is_reversible_with_pre_restore_checkpoint`
- `delete_hides_checkpoint_from_public_commands`

### 2. Worktree 专属存储

草案要求：

- 当前 worktree 拥有独立 Fossil repository。
- 不从 common Git directory 共享 checkpoint。
- 不跨 worktree list / restore。
- linked worktree 根部 `.git` gitfile 不应被当作目录。

当前状态：**完整实现**。

实现情况：

- 使用 `git rev-parse --git-dir` 定位当前 worktree 专属 Git admin dir。
- 存储路径为 `<worktree-git-dir>/git-chkpt/`。
- 主 worktree 与 linked worktree 的 `repository.fossil` 物理隔离。
- 不提供 worktree 选择参数或 registry。

测试覆盖：

- `linked_worktrees_have_isolated_checkpoint_storage`

### 3. `.git` 生命周期从属

草案要求：删除 `.git` 或重新初始化后，不搜索或继承旧 checkpoint。

当前状态：**完整实现**。

实现情况：

- checkpoint 存储位于当前 Git admin dir 下。
- `.git` 被删除后，旧存储自然消失或不可达。
- 重新 `git init` 后，`list` 返回空，不重新关联旧 checkpoint。

测试覆盖：

- `checkpoints_follow_git_directory_lifecycle`

### 4. 本地 Fossil 后端

草案要求：

- 使用 Fossil SCM。
- 每个 checkpoint 对应 Fossil check-in。
- 关闭 autosync。
- 不配置 remote，不执行网络同步。

当前状态：**完整实现**。

实现情况：

- `repository.fossil` 是唯一内容数据库。
- `checkout/` 是私有 Fossil checkout。
- 初始化执行 `fossil settings autosync off`。
- commit 使用 `--private --nosync`。
- 代码没有 push / pull / sync / clone 路径。

### 5. 基础 ID 解析

草案要求：

- 完整 hash 精确匹配优先。
- 短前缀必须唯一。
- 无匹配返回 not-found。
- 多匹配返回 ambiguous。
- 不跨 worktree 搜索。
- 不用时间、序号、消息做模糊匹配。

当前状态：**完整实现**。

实现情况：

- `resolve_checkpoint` 实现完整 ID 与前缀匹配。
- `list` 展示当前列表中唯一的最短前缀，至少 8 字符。
- 只从当前 worktree store 的 Fossil timeline 加载 checkpoint。

测试覆盖：

- 当前只间接覆盖 ID 使用；`ambiguous-checkpoint` 尚缺专门测试。

### 6. 非目标约束

草案列出的非目标包括：

- 自动监听 / 定时 checkpoint。
- 云同步 / remote checkpoint。
- GUI / IDE 插件。
- 多人协作 / checkpoint 分享。
- import / export。
- 跨设备恢复。
- 单文件恢复。
- Git HEAD / branch / index / staged 状态恢复。
- merge / rebase / cherry-pick 状态恢复。
- 可插拔后端。

当前状态：**完整遵守**。

实现情况：

- 当前代码没有引入后端 registry、抽象插件层、worktree registry、remote/sync 入口或 GUI 逻辑。
- 当前实现明确只做当前 worktree 文件世界 checkpoint。

## 基本实现

### 7. Save 无副作用

草案要求：`save` 不修改用户 worktree 文件，不修改 Git-owned state，只写私有目录。

当前状态：**基本实现**。

已实现：

- save 只写 `<git-dir>/git-chkpt/` 私有目录。
- 不写用户文件。
- 不主动写 Git index / HEAD / refs。
- save 失败会尝试清理 staging。
- save 使用 lock，避免并发写 Fossil checkout。

测试覆盖：

- `save_does_not_modify_git_status_or_index`

仍欠缺：

- 未逐字节覆盖 HEAD、refs、hooks、Git config、sequencer 等所有 Git-owned state。
- 未测试 symlink 在 save 前后完全一致。

### 8. Managed universe

草案定义：

```text
Git tracked paths + Git 未忽略的 untracked paths
```

排除 ignored、`.git`、submodule、nested repository、私有存储、worktree 外内容。

当前状态：**基本实现**。

已实现：

- tracked 使用 `git ls-files --stage -z`。
- untracked 使用 `git ls-files -o --exclude-standard -z`。
- ignored 文件不进入 manifest。
- submodule gitlink mode `160000` 被视为边界。
- nested Git repository 通过 `.git` 边界识别并排除。
- 当前实现只保存实际存在的文件或 symlink；已删除 tracked 文件不进入当前快照。

测试覆盖：

- `save_diff_restore_preserves_ignored_files`
- `nested_git_repository_is_not_snapshotted_or_restored`
- `parent_checkpoint_does_not_enter_real_submodule`

仍欠缺：

- `.gitignore` 在 save / restore 期间变化时的最终语义未完全定义。
- 文件名编码和复杂平台路径未完整验证。

### 9. Checkpoint manifest

草案要求 manifest 自描述、UTF-8、相对路径、排序、去重、hash、UTC 时间、不含 Git 内部状态。

当前状态：**基本实现**。

已实现：

- `format_version = 1`。
- UTC 创建时间。
- source / message / files。
- 普通文件记录 mode、size、SHA-256。
- symlink 记录 target。
- path validation：拒绝绝对路径、`..`、NUL、`.git`、重复路径、ancestor 冲突。
- case-folding 冲突拒绝。
- 不记录 HEAD / branch / index / Git commit / remote / username / hostname / worktree 绝对路径。

仍欠缺：

- case-folding 冲突目前是无条件拒绝，不区分目标文件系统是否大小写敏感。
- manifest 不显式记录目录，因此不保存空目录。
- 未记录 symlink 目标是文件还是目录，Windows symlink 恢复语义不完整。

### 10. 完整性校验

草案要求 restore 前校验 Fossil check-in、manifest、文件存在、类型、size、SHA-256、额外内容、逃逸路径、大小写冲突、危险 symlink。

当前状态：**基本实现**。

已实现：

- manifest 可读取和 parse。
- manifest version/path/metadata 校验。
- Fossil payload 路径集合必须等于 manifest 推导集合。
- 普通文件 materialize 后校验 size 和 SHA-256。
- materialized 目录不允许出现 manifest 未声明普通文件。
- restore 写入前检查 symlink ancestors。
- restore 使用 `join_under` 防止路径逃逸。

仍欠缺：

- corrupt checkpoint 现在被公开命令静默跳过，缺少用户可见诊断入口。
- symlink 安全语义只保存 link 本身，但未充分测试所有平台危险 reparse/symlink 情况。
- Fossil check-in 时间与 manifest 时间差异未校验。

### 11. Restore 可逆与 rollback

草案要求：restore 修改工作区前必须成功创建 pre-restore checkpoint；失败后 rollback。

当前状态：**基本实现**。

已实现：

- restore 在应用目标前执行内部 pre-restore save。
- pre-restore checkpoint 使用同一 manifest 和 Fossil 存储机制。
- restore 成功后输出 pre-restore 和目标 checkpoint ID。
- restore 失败后尝试 materialize pre-restore 并 rollback。
- rollback 成功时报告 restore 失败但工作区已恢复。
- rollback 失败时保留 journal 和 materialization 目录。

测试覆盖：

- `restore_is_reversible_with_pre_restore_checkpoint`

仍欠缺：

- rollback 失败路径没有自动化测试覆盖。
- pre-restore 失败的错误类别未单独命名为 `pre-restore-failed`。

### 12. Restore 完整恢复

草案要求：目标存在的必须恢复；目标不存在的当前受管理路径必须删除；文件/目录类型变化必须正确恢复；ignored 不删除。

当前状态：**基本实现**。

已实现：

- 目标文件创建/替换。
- 目标 symlink 创建。
- 目标不存在的当前 managed path 删除。
- ignored 文件保留。
- 文件/目录类型切换恢复。
- 最终重新 capture 并比较 manifest。

测试覆盖：

- `save_diff_restore_preserves_ignored_files`
- `restore_preserves_ignored_children_inside_deleted_dirs`
- `restore_handles_file_directory_type_changes`

仍欠缺：

- 空目录不保存，所以“完整文件世界”不包含空目录。
- restore plan 没有作为显式数据结构持久化。
- 当前实现先删除 stale paths，再恢复目标文件；未完全达到“先准备全部待写文件，再执行计划”的强事务形态。

### 13. 初始化语义

草案要求：首次写操作可初始化；只读命令未初始化时不创建 repository。

当前状态：**基本实现**。

已实现：

- `save` 和 `restore` 会初始化 Fossil store。
- `list` 未初始化时成功空输出。
- `show` / `diff` 未初始化时返回 `no-checkpoint`。
- 初始化不修改用户 worktree 或 Git-owned state。

测试覆盖：

- `read_commands_do_not_initialize_checkpoint_storage`
- `default_command_saves_checkpoint`

备注：

- `delete` 在未初始化时返回 `no-checkpoint`，不会初始化。这符合当前实现规格；草案把 delete 放入“首次执行需要写操作的命令”列表，但没有 checkpoint 时初始化没有实际价值。

### 14. 输出原则

草案要求默认输出克制，不能出现 HEAD / branch / force 等错误心智模型。

当前状态：**基本实现**。

已实现：

- save 输出短 ID 和 message。
- restore 输出 pre-restore 和 target ID。
- list 输出 ID / CREATED / SOURCE / MESSAGE。
- show 输出 metadata 摘要。
- 不因 HEAD / branch / commit 差异警告或拒绝。
- 不要求 `--force`。

仍欠缺：

- 错误消息不总是明确说明“是否修改了工作区”。
- 底层路径和 Fossil 错误可能在普通错误中出现，尚未区分诊断模式。

## 部分实现

### 15. Diff

草案 SHOULD：

- 文本文件显示 unified diff。
- 二进制显示 binary changed。
- 表达新增、删除和类型变化。
- 不修改 Fossil checkout 持久状态。
- 不修改工作区。

当前状态：**部分实现**。

已实现：

- `diff [CHECKPOINT]` 存在。
- 省略 ID 时比较最新 checkpoint。
- 输出方向是 `checkpoint -> current workspace`。
- 输出 `A` / `M` / `D` 摘要。
- 尊重 managed universe，ignored / submodule 不进入比较。
- 不修改工作区。

未实现：

- unified diff。
- binary changed。
- 单独的 type-change 标记。
- diff 退出码策略。
- checkpoint A vs checkpoint B 比较。

### 16. 并发模型

草案要求独占锁/shared lock，busy 时不修改状态并显示 holder 信息。

当前状态：**部分实现**。

已实现：

- `save/restore/delete` 使用 exclusive file lock。
- `list/show/diff` 使用 shared file lock。
- lock 覆盖 Fossil checkout、commit、delete、restore journal、pre-restore、工作区恢复。
- lock metadata 包含 pid、operation、started_at_utc。
- busy 错误会尽力显示 holder 信息。

未完全实现：

- 为了创建 lock 文件，第一次加锁仍可能创建私有 lock parent；这是文件锁实现的现实折中。
- stale metadata / PID reuse 没有完整算法。
- lock contention 没有自动化测试。

### 17. Restore crash recovery

草案要求下一次命令发现 incomplete restore journal 后阻止普通操作，并尽可能恢复 pre-restore。

当前状态：**部分实现**。

已实现：

- `active-restore.json` journal。
- journal 记录 target、pre-restore、stage、status、diagnostic。
- 普通 save/delete/read 遇到 in-progress journal 会拒绝继续。
- `restore` 遇到 in-progress journal 会尝试恢复 pre-restore。
- 自动恢复成功后提示用户重跑原命令。
- 自动恢复失败保留诊断材料。

未实现：

- 精确状态机。
- 每个 restore step 的持久化 plan。
- crash injection 测试。
- 各阶段 crash 后的强恢复证明。

### 18. Git 状态独立验收

草案最低验收要求在普通 branch、detached HEAD、staged/unstaged、merge、rebase、cherry-pick 中分别 save/restore。

当前状态：**部分实现**。

已实现：

- save 不主动写 Git index / HEAD / refs。
- restore 不主动写 Git index / HEAD / refs。
- 有 index 字节不变测试。
- 不因 branch / HEAD / commit 不同而检查或拒绝。

未测试 / 未完整证明：

- detached HEAD。
- staged 与 unstaged 同时存在。
- merge 进行中。
- rebase 进行中。
- cherry-pick 进行中。
- sequencer state 字节不变。

### 19. 文件类型与平台特殊项

草案初始类型包括普通文件、目录、symbolic link，并列出特殊文件、reparse point、hard link 等原则。

当前状态：**部分实现**。

已实现：

- 普通文件保存和恢复。
- Symlink 保存 link target，而不是跟随读取目标内容。
- 普通目录作为路径容器参与恢复。
- 文件/目录类型切换恢复。
- 特殊设备、socket、FIFO 等非普通文件和非 symlink 默认拒绝保存。
- Unix executable bit 可通过 `100755` 保存/恢复。

未实现 / 未完全定义：

- 空目录保存。
- 显式目录 entry。
- hard link 关系保存。
- Windows directory symlink 区分。
- junction / reparse point 安全策略。
- Windows executable bit 语义。

### 20. 错误模型

草案列出错误类别，并要求人类可读、说明是否修改工作区、restore 失败说明 rollback 状态、不输出虚假成功。

当前状态：**部分实现**。

已实现：

- 主要错误以字符串前缀表达：`no-checkpoint`、`checkpoint-not-found`、`ambiguous-checkpoint`、`workspace-changed`、`corrupt-checkpoint`、`restore-failed`、`rollback-failed`、`incomplete-transaction` 等。
- restore 成功 / 失败路径不会输出虚假成功。
- rollback 成功 / 失败在错误中区分。

未实现：

- 稳定结构化错误类型。
- 退出码映射。
- 诊断模式。
- 所有错误都明确说明工作区是否已修改。
- 底层路径隐藏策略。

## 未实现

### 21. Unified diff / binary diff

草案中 `diff` 的增强展示未实现：

- 文本 unified diff。
- binary changed。
- 类型变化专门标记。
- 二进制大小/hash 展示。
- diff 退出码策略。

当前只有 A/M/D path summary。

### 22. 完整 restore plan 数据结构

草案要求 restore plan 至少包含：

```text
create
replace
delete
type-change
mkdir
rmdir-if-empty
```

当前没有显式 plan 数据结构，也没有持久化每步 plan。

实际实现是：

1. 扫描当前 manifest。
2. 计算 stale paths。
3. 删除 stale managed paths。
4. 遍历目标 entries 并写入。
5. 最终 verify。

### 23. 完整 crash injection 验收

草案 27.8 要求在多个 restore 阶段注入崩溃：

- pre-restore 前。
- pre-restore 后。
- staging 后。
- 第一个文件替换后。
- 部分删除后。
- 最终校验前。
- journal 完成前。

当前没有 crash injection 测试框架。

### 24. Corrupt checkpoint 用户诊断

草案说损坏 checkpoint 不得伪装为正常 checkpoint，并开放是否默认显示损坏项。

当前实现：

- list/show/diff/restore 只处理校验通过的 checkpoint。
- 损坏 checkpoint 会被跳过。

未实现：

- 列出 corrupt checkpoint。
- 诊断 corrupt 原因。
- 删除 corrupt checkpoint 的专门入口。

### 25. 物理删除 / 空间回收

草案将 delete 物理删除列为开放问题。

当前实现：

- 只做逻辑删除。
- 不重写 Fossil repository。
- 不回收物理空间。

物理删除未实现，也不建议作为 v1 默认承诺。

### 26. JSON 输出

草案开放问题提到是否公开 `--json`。

当前未实现任何 `--json` 输出。

### 27. 空目录 checkpoint

草案尚未决定是否记录目录和保留空目录。

当前未实现空目录保存。

### 28. 超大文件策略

草案倾向不设任意限制，但要求处理磁盘空间不足。

当前没有显式大文件限制或专门的磁盘空间不足策略。

### 29. Git 操作状态完整不变性测试

当前没有覆盖：

- HEAD 字节/refs 字节完整对比。
- hooks/config 不变性测试。
- sequencer/merge/rebase/cherry-pick 状态不变性测试。

## 开放问题与当前临时选择

### 30. `.gitignore` 变化语义

草案开放：

- `.gitignore` 在 save 和 restore 期间变化时如何定义。
- restore 开始时还是 pre-restore save 时决定 ignored 集合。

当前实现：

- 每次 capture 都通过 Git 现场规则重新枚举。
- restore 最终 verify 也基于当前工作区规则。

结论：**开放问题，当前只有实现行为，没有产品级最终定义。**

### 31. Submodule 根路径表达

草案开放：

- submodule 根目录本身是否进入 manifest。
- gitlink 是否作为 boundary 记录。
- restore 是否完全忽略 submodule 路径。

当前实现：

- 父项目完全不进入 submodule 内部。
- 当前 manifest 不记录 submodule boundary entry。
- restore 不修改 submodule 内部 dirty 内容。

结论：**开放问题，当前选择是“忽略内部，不显式记录 boundary”。**

### 32. Fossil checkout 长期保留

草案开放：长期保留私有 checkout，还是每次临时创建。

当前实现：

- 长期保留 `<git-dir>/git-chkpt/checkout/`。

结论：**当前采用长期保留，后续可重新评估。**

### 33. Fossil check-in payload 形态

草案开放：直接包含 manifest 与项目文件，还是封装 `files/` 目录。

当前实现：

```text
manifest.json
files/<path>
```

结论：**当前采用 `files/` 封装。**

### 34. 重复空内容 checkpoint

草案初步倾向：每次显式 save 都创建 checkpoint。

当前实现：

- 每次 save 都生成新 manifest 时间戳并 Fossil commit。
- 使用 `--allow-empty`。

结论：**基本实现，但缺少专门测试。**

### 35. Bare repository

草案开放问题里当前答案为不支持。

当前实现：

- bare repository 返回 `bare-repository`。

结论：**已按当前答案实现。**

## 当前验收覆盖对照

| Draft 验收项 | 当前状态 | 测试 |
|---|---:|---|
| 27.1 Save 无副作用 | 部分覆盖 | `save_does_not_modify_git_status_or_index` |
| 27.2 完整恢复 | 完整覆盖基础场景 | `save_diff_restore_preserves_ignored_files` |
| 27.3 Restore 可逆 | 完整覆盖基础场景 | `restore_is_reversible_with_pre_restore_checkpoint` |
| 27.4 Git 状态独立 | 部分覆盖 | `restore_does_not_modify_git_index_bytes` |
| 27.5 Worktree 隔离 | 完整覆盖基础场景 | `linked_worktrees_have_isolated_checkpoint_storage` |
| 27.6 `.git` 生命周期 | 完整覆盖基础场景 | `checkpoints_follow_git_directory_lifecycle` |
| 27.7 Submodule 隔离 | 完整覆盖基础场景 | `parent_checkpoint_does_not_enter_real_submodule` |
| 27.8 崩溃恢复 | 未覆盖 | 无 crash injection tests |

## 推荐后续实现顺序

### P0：把部分实现变成完整实现

1. 增加 crash injection 测试框架。
2. 把 restore journal 扩展为精确状态机。
3. 增加 merge/rebase/cherry-pick/detached HEAD 状态独立测试。
4. 增加 symlink save/restore 测试，尤其是 Windows 行为。
5. 增加 corrupt checkpoint 诊断命令或诊断模式。

### P1：补用户体验能力

1. unified diff。
2. binary changed 输出。
3. type-change 独立标记。
4. `--json` 输出。
5. 更清晰的错误码与退出码。

### P2：解决开放问题

1. `.gitignore` 变化语义。
2. 空目录是否保存。
3. submodule boundary 是否入 manifest。
4. hard link / reparse point 策略。
5. delete 是否永远保持逻辑删除。

## 结论

当前实现已经覆盖产品草案的核心使用闭环：

```text
save -> list/show/diff -> restore -> pre-restore 反悔 -> delete
```

也覆盖了最重要的安全边界：

```text
per-worktree isolation
ignored preservation
.git exclusion
submodule/nested repo exclusion
no Git-owned state restoration
```

但 `PRODUCT_SPEC_DRAFT.md` 仍包含一些未落地的长期目标，尤其是：

```text
完整 crash recovery 状态机
unified diff
完整 Git 操作状态验收
平台特殊文件语义
corrupt checkpoint 诊断
```

因此当前项目文档应这样理解：

- `IMPLEMENTATION_SPEC.md` 是当前真实契约。
- `PRODUCT_SPEC_DRAFT.md` 是目标草案。
- 本文是两者之间的覆盖矩阵。
