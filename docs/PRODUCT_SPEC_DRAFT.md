
**git-chkpt Specification**

状态：Draft v0.1  
日期：2026-08-16  
规范语言：中文  
规范关键词：MUST、MUST NOT、SHOULD、SHOULD NOT、MAY

**1. 概述**

`git-chkpt` 是一个 Git 外部子命令，用于保存、查看、比较、恢复和删除当前 Git worktree 的文件 checkpoint。

Checkpoint 是当前 worktree 中受管理文件的完整内容快照。

```bash
git chkpt save
git chkpt list    # alias: ls
git chkpt show
git chkpt diff
git chkpt restore
git chkpt delete  # alias: rm
```

`git-chkpt` 使用 Fossil SCM 作为本地 checkpoint 存储后端；支持系统 `PATH`、随包 sidecar 或按需自动获取官方 Fossil 预编译包，用户不应需要强制单独安装系统级 `fossil`。

Fossil repository 属于当前 worktree，存储在该 worktree 专属 Git administrative directory 下的私有命名空间中。

`git-chkpt` 不保存、不恢复、不解释 Git 的内部状态。

**2. 产品目标**

`git-chkpt` MUST 解决以下问题：

> 用户的工作内容尚未形成正式 Git commit，但用户希望在不中断当前工作的情况下，立即保存一个可以完整恢复的文件现场。

Checkpoint 必须具备以下特征：

- 创建成本低。
- 不要求整理 commit。
- 不要求创建 branch。
- 不会像 stash 一样收起或改变当前工作区。
- 可以连续创建任意多个。
- 可以随时删除。
- 可以完整恢复。
- restore 前自动保存当前现场，因此恢复操作本身可逆。
- checkpoint 不进入 Git 正式历史。
- checkpoint 不需要具备长期保存价值。

产品心智模型：

```text
Git commit
    项目的正式历史，属于“岁月史书”。

Git checkpoint
    正式历史写下之前的临时现场，属于“安全存档”。
```

Checkpoint MAY 在相应工作形成正式 commit 后失去价值。用户删除此类 checkpoint 是正常工作流，不应被视为需要警告的危险操作。

**3. 非目标**

以下能力不属于当前版本，也不属于项目未来规划中的默认方向：

- 自动文件监听。
- 定时 checkpoint。
- 云同步。
- remote checkpoint。
- GUI。
- IDE 插件。
- 多人协作。
- checkpoint 分享。
- checkpoint 导入或导出。
- 跨设备恢复。
- 独立于 Git repository 的灾难备份。
- `.git` 删除后的 checkpoint 恢复。
- Git repository 重新初始化后的 checkpoint 继承。
- 跨 worktree checkpoint 查看或恢复。
- 单文件或部分路径恢复。
- Git HEAD 恢复。
- Git branch 恢复。
- Git index 恢复。
- staged 与 unstaged 边界恢复。
- merge、rebase、cherry-pick 或 sequencer 状态恢复。
- submodule 内容的递归 checkpoint。
- 可插拔存储后端。

实现 MUST NOT 为上述假想能力预先引入额外抽象、registry、项目身份管理或后端接口层。

**4. 核心术语**

**4.1 Worktree**

本规范中的 worktree 是当前命令所作用的 Git working tree。

一个 Git repository 可以拥有一个主 worktree 和零个或多个 linked worktree。Linked worktree 共享 Git object store 和大部分 repository 数据，但拥有独立的当前文件世界以及独立的 `HEAD`、index 等 per-worktree 状态。[5][6]

`git-chkpt` 的操作对象是当前 worktree，不是多个 worktree 共享的 Git repository。

**4.2 Git administrative directory**

当前 worktree 对应的 Git 管理目录。

实现 MUST 通过 Git 命令解析此目录，MUST NOT 假定 worktree 根目录下的 `.git` 一定是目录。

在 linked worktree 和 submodule 中，worktree 根目录下的 `.git` 可以是指向实际管理目录的普通文本文件。[7][8]

**4.3 Managed universe**

Checkpoint 管理的文件集合，称为 managed universe。

逻辑定义：

```text
managed universe
=
Git tracked paths
+
Git 未忽略的 untracked paths
```

以下内容永远不属于 managed universe：

- 当前 Git administrative directory。
- `.git` 目录或 gitfile。
- ignored 文件和目录。
- 嵌套 Git repository 的内部内容。
- submodule 内部内容。
- `git-chkpt` 私有存储。
- worktree 根目录外的任何内容。

**4.4 Checkpoint**

一次完整的 managed universe 快照。

Checkpoint 包含：

- 创建时间。
- 可选消息。
- 来源。
- 格式版本。
- 完整路径 manifest。
- 每个路径的类型和必要元数据。
- 每个普通文件的内容。
- 完整性校验信息。

Checkpoint 不包含：

- HEAD。
- branch。
- ref。
- Git commit ID。
- index。
- staged 状态。
- Git config。
- reflog。
- hook。
- Git object。
- rebase 或 merge 状态。
- worktree 绝对路径。
- 用户机器身份。

**4.5 Pre-restore checkpoint**

执行 restore 前，工具自动保存的当前 managed universe 完整快照。

Pre-restore checkpoint 与手动 checkpoint 使用相同的数据格式和存储机制，来源字段会标明它是自动生成，并记录触发它的命令。

它的目的不是审计，而是确保 restore 可逆。

**5. 核心不变量**

以下约束适用于所有实现。

**5.1 Save 无副作用**

`git chkpt save` 无论成功或失败，MUST NOT 修改：

- worktree 中的任何用户文件。
- Git HEAD。
- Git index。
- Git refs。
- Git config。
- Git objects。
- Git hooks。
- Git sequencer 状态。
- Git merge 或 rebase 状态。
- Git 管理的其他 repository 状态。

`save` MAY 读取 Git metadata，以确定：

- worktree 根目录。
- 当前 worktree 的 Git administrative directory。
- tracked 路径。
- ignored 路径。
- submodule 或嵌套 repository 边界。

`save` 只能写入 `git-chkpt` 自己拥有的私有目录。

**5.2 Restore 不修改 Git-owned state**

`restore` MUST 修改 managed universe 中的工作区文件，使其与目标 checkpoint 一致。

`restore` MUST NOT 主动修改任何 Git-owned state。

恢复工作区文件后，Git 对这些文件的观察结果可能变化。例如 `git status`、staged 或 unstaged 差异可能改变。这不属于对 Git index 的修改，也不属于 restore 失败。

**5.3 完整恢复**

Restore 不是覆盖式解包。

成功 restore 后，managed universe MUST 与目标 checkpoint 的 manifest 完全一致：

- 目标中存在的路径必须存在。
- 目标中的文件内容必须一致。
- 目标中的 symlink 目标必须一致。
- 目标中的必要文件模式必须一致。
- 目标中不存在的当前受管理路径必须删除。
- 文件与目录之间的类型变化必须正确恢复。

**5.4 Ignored 路径不受管理**

Ignored 路径：

- MUST NOT 被 save。
- MUST NOT 被 restore。
- MUST NOT 被 restore 删除。
- MUST NOT 被完整性验证要求与 checkpoint 一致。

原因是 ignored 内容没有进入 pre-restore checkpoint，工具不能承诺其可逆性。

**5.5 `.git` 不受管理**

Git administrative data：

- MUST NOT 被 save。
- MUST NOT 被 restore。
- MUST NOT 被删除。
- MUST NOT 出现在 checkpoint manifest 中。

**5.6 Restore 必须可逆**

任何 restore 在修改用户工作区之前，MUST 成功创建 pre-restore checkpoint。

如果 pre-restore checkpoint 创建失败，restore MUST 中止，且 MUST NOT 修改工作区。

**5.7 Checkpoint 生命周期**

Checkpoint 的生命周期从属于当前 Git worktree。

删除以下内容 MAY 同时删除 checkpoint：

- 当前 worktree。
- 当前 worktree 的 Git administrative directory。
- 整个 `.git`。
- 整个项目目录。

`git-chkpt` 不承担 `.git` 被删除或替换后的 checkpoint 恢复责任。

**5.8 本地性**

所有 checkpoint 数据 MUST 保存在本机。

实现 MUST：

- 禁用 Fossil autosync。
- 不配置 Fossil remote。
- 不执行 push、pull、sync 或 clone。
- 不提供隐式网络访问。
- 不因 Fossil repository 中存在 remote 配置而自动访问网络。

**6. Worktree 模型**

每个 worktree MUST 拥有独立的 Fossil repository 和独立的 checkpoint 时间线。

概念结构：

```text
Shared Git repository
├── shared Git objects and refs
│
├── worktree A
│   ├── file world A
│   └── checkpoint history A
│
└── worktree B
    ├── file world B
    └── checkpoint history B
```

在哪个 worktree 中执行 `git chkpt`，就只能访问该 worktree 的 checkpoint。

实现 MUST NOT：

- 从 common Git directory 共享 checkpoint repository。
- 列出其他 worktree 的 checkpoint。
- 恢复其他 worktree 的 checkpoint。
- 提供 worktree 选择参数。
- 维护 worktree 路径 registry。
- 猜测两个 worktree 是否应共享 checkpoint。

**6.1 存储目录解析**

实现 MUST 使用 Git 命令获得：

```bash
git rev-parse --show-toplevel
git rev-parse --git-dir
```

返回的相对路径必须相对于命令执行目录或 Git 定义的正确基准解析，并最终规范化为绝对路径。

实现 SHOULD 使用 Git 提供的路径解析能力，避免自行解析 `.git` gitfile。

实现 MUST NOT 使用以下假设：

```text
git_dir = worktree_root / ".git"
```

**6.2 主 worktree**

典型存储位置：

```text
project/
└── .git/
    └── git-chkpt/
        ├── repository.fossil
        ├── checkout/
        ├── staging/
        ├── transactions/
        └── lock
```

**6.3 Linked worktree**

典型存储位置：

```text
main-project/
└── .git/
    └── worktrees/
        └── feature/
            └── git-chkpt/
                ├── repository.fossil
                ├── checkout/
                ├── staging/
                ├── transactions/
                └── lock
```

Linked worktree 根部的 `.git` gitfile MUST NOT 被当作目录拼接路径。

**6.4 Worktree 删除**

当用户通过 Git 删除 linked worktree 时，该 worktree 的 checkpoint MAY 随其 administrative directory 一并删除。

如果用户直接删除 linked worktree 目录，Git 可能暂时保留 stale worktree administrative state，并在后续通过 `git worktree prune` 清理。此生命周期属于 Git，不属于 `git-chkpt` 的 orphan 管理。[5][6]

`git-chkpt` MUST NOT 实现：

- orphan worktree 检测。
- checkpoint attach。
- checkpoint move。
- checkpoint worktree repair。
- stale worktree prune。

**7. Submodule 与嵌套 repository**

顶层 checkpoint MUST NOT 递归进入 submodule 或其他嵌套 Git worktree。

假设：

```text
project/
├── .git/
├── src/
└── vendor/libfoo/
    ├── .git
    └── src/
```

在 `project/` 执行：

```bash
git chkpt save
```

不得保存 `vendor/libfoo/` 内部文件内容。

在 submodule 根目录执行：

```bash
cd vendor/libfoo
git chkpt save
```

则该 submodule 作为独立 Git worktree，拥有自己的 checkpoint repository。

父项目 checkpoint MAY 记录 submodule 路径作为 opaque boundary，但不得进入其内部。

开放问题：

- submodule 根目录本身是否进入 manifest。
- submodule gitlink 在工作区快照中是否仅作为边界记录。
- restore 时是否完全忽略 submodule 路径，还是只保证不删除其根目录。

初步倾向：完整忽略 submodule 内部，并将其根目录视为不可删除边界。

**8. 存储布局**

`git-chkpt` 私有目录：

```text
<worktree-git-dir>/git-chkpt/
```

建议布局：

```text
git-chkpt/
├── repository.fossil
├── checkout/
├── staging/
├── transactions/
├── lock
└── version
```

各目录职责：

- `repository.fossil`
  - 唯一 checkpoint 内容数据库。
  - 普通 Fossil repository。
  - 不配置 remote。
  - 禁用 autosync。

- `checkout/`
  - 私有 Fossil checkout。
  - 不得指向用户工作区。
  - 不得包含用户 worktree 的绝对路径作为业务身份。

- `staging/`
  - save 和 restore 的临时物化目录。
  - 只有当前成功事务可使用。
  - 不得作为 checkpoint 真相来源。

- `transactions/`
  - restore journal。
  - 崩溃恢复信息。
  - 未完成事务记录。

- `lock`
  - 当前 worktree 的 checkpoint 操作锁。

- `version`
  - 可选的私有目录布局版本。
  - 不替代 checkpoint manifest 的 `format_version`。

**9. Fossil 使用约束**

Fossil 是唯一支持的 checkpoint 存储后端。

实现 MAY 直接调用系统 PATH 中的 Fossil CLI、随包发布的 Fossil CLI sidecar 或自动下载/缓存的 Fossil CLI，也 MAY 使用稳定的 Fossil 接口，但 MUST 保持下列语义：

- 每个完整 checkpoint 对应一个 Fossil check-in。
- Fossil check-in hash 是 checkpoint 的内部完整 ID。
- 保存相同内容仍 MAY 创建新的 checkpoint，因为创建时间、来源或消息不同。
- Fossil branch、tag、wiki、ticket、forum 等能力不属于本工具。
- 工具不得依赖用户理解 Fossil 工作流。
- 用户直接打开 `fossil ui` 不受禁止。
- 用户直接修改 Fossil repository 不属于 `git-chkpt` 契约。
- 即使 repository 被外部修改，restore 仍 MUST 执行完整性校验。
- 外部修改导致 checkpoint 不符合本规范时，工具 MUST 拒绝危险恢复。

实现 MUST 显式关闭 autosync。Fossil 的 autosync 设置能够影响 commit 等命令，因此不得依赖默认值。[6]

**10. Checkpoint ID**

Checkpoint 的完整 ID MUST 为对应 Fossil check-in 的完整 hash。

命令 MAY 接受唯一 hash 前缀：

```bash
git chkpt show 4f18ac9
git chkpt restore 4f18ac9
git chkpt delete 4f18ac9
git chkpt rm 4f18ac9
```

解析规则：

1. 完整 hash 精确匹配优先。
2. 短前缀必须在当前 worktree Fossil repository 内唯一。
3. 无匹配时返回 not-found 错误。
4. 多匹配时返回 ambiguous-ID 错误。
5. 不得跨 worktree 搜索。
6. 不得使用时间、序号或消息进行隐式模糊匹配。

展示 ID SHOULD 使用足以在当前 repository 内保持唯一的最短前缀。

展示长度 SHOULD 至少为 8 个字符。

输出 MAY 同时提供完整 ID，例如 JSON 输出中始终使用完整 ID。

**11. Checkpoint manifest**

每个 Fossil check-in MUST 包含一个由 `git-chkpt` 管理的自描述 manifest。

概念结构：

```json
{
  "format_version": 1,
  "created_at_utc": "2026-08-15T16:04:21.123456Z",
  "source": {
    "kind": "manual",
    "operation": "save"
  },
  "message": "parser works before cleanup",
  "files": [
    {
      "path": "src/parser.rs",
      "type": "file",
      "mode": "100644",
      "size": 18342,
      "sha256": "..."
    },
    {
      "path": "scripts/run",
      "type": "symlink",
      "target": "./run-linux"
    }
  ]
}
```

Pre-restore checkpoint 示例：

```json
{
  "format_version": 1,
  "created_at_utc": "2026-08-15T16:10:00.123456Z",
  "source": {
    "kind": "automatic",
    "operation": "pre-restore",
    "target_checkpoint": "4f18ac932e7d..."
  },
  "message": null,
  "files": []
}
```

Manifest MUST：

- 使用 UTF-8。
- 使用 `/` 作为逻辑路径分隔符。
- 只包含相对 worktree 根目录的路径。
- 拒绝绝对路径。
- 拒绝 `..` 路径逃逸。
- 拒绝 NUL。
- 拒绝重复逻辑路径。
- 拒绝大小写归一化后冲突的路径，具体取决于目标文件系统。
- 按确定性顺序排列 paths。
- 包含 `format_version`。
- 包含 UTC 创建时间。
- 包含每个普通文件的 SHA-256。
- 足以单独确定目标 managed universe。

Manifest SHOULD 不记录：

- worktree 绝对路径。
- 用户名。
- 主机名。
- branch。
- HEAD。
- index。
- Git remote。
- Git repository identity。

**12. 路径和文件类型**

初始支持的路径类型：

- 普通文件。
- 目录。
- symbolic link。

开放问题：

- 是否显式记录目录。
- 是否保留空目录。
- Windows symlink 权限与恢复失败语义。
- executable bit 在 Windows 上如何处理。
- hard link 是否还原为独立文件，还是保留链接关系。
- junction、reparse point 和其他平台特殊文件如何处理。

初步原则：

- 特殊设备、socket、FIFO 等 MUST 默认拒绝保存。
- 不得静默跟随 symlink 读取 worktree 外内容。
- Symlink MUST 保存链接本身，而不是其目标内容。
- 无法安全识别的 reparse point MUST 导致 save 失败，不能静默递归。
- 文件权限只保存跨平台有明确意义的部分。
- v1 SHOULD 保存 Git 相关的 executable bit 语义。
- 空目录是否保存需在实现前确定。

**13. 并发模型**

每个 worktree checkpoint repository 必须有独立互斥锁。

以下命令 MUST 获取独占锁：

- `save`
- `restore`
- `delete`

以下命令 SHOULD 获取共享锁或使用等价的一致读取机制：

- `list`
- `show`
- `diff`

锁 MUST 覆盖：

- Fossil checkout 修改。
- Fossil commit。
- checkpoint 删除。
- restore staging。
- restore journal。
- pre-restore checkpoint。
- 工作区恢复过程。

如果无法获取锁，命令 MUST：

- 不修改任何状态。
- 返回明确的 busy 错误。
- 显示持锁操作信息，如果可安全获得。
- 不通过删除锁文件的方式擅自破锁。

锁实现必须考虑：

- 进程崩溃。
- PID 重用。
- Windows 文件锁。
- Unix advisory lock。
- stale metadata。

具体锁算法留待实现章节细化。

**14. 公共命令**

公开命令限定为：

```bash
git chkpt save [MESSAGE]
git chkpt list    # alias: ls
git chkpt show [CHECKPOINT]
git chkpt diff [CHECKPOINT]
git chkpt restore [CHECKPOINT]
git chkpt delete <CHECKPOINT>...  # alias: rm
```

快捷形式：

```bash
git chkpt
```

MUST 等价于：

```bash
git chkpt save
```

命令别名暂不纳入规范。

**15. `git chkpt save [MESSAGE]`**

创建当前 managed universe 的完整 checkpoint。

示例：

```bash
git chkpt save
git chkpt save "parser works before cleanup"
git chkpt
```

成功输出建议：

```text
Saved checkpoint 4f18ac932e7d
```

带消息时：

```text
Saved checkpoint 4f18ac932e7d
Message: parser works before cleanup
```

**15.1 Save 算法**

实现 MUST 按以下逻辑执行：

1. 确认当前目录位于非 bare Git worktree 中。
2. 解析 worktree 根目录。
3. 解析当前 worktree 专属 Git administrative directory。
4. 确定 `git-chkpt` 私有目录。
5. 获取独占锁。
6. 初始化或打开 Fossil repository。
7. 确认 autosync 已关闭，且当前操作不会访问网络。
8. 创建空的 staging 目录。
9. 使用 Git 只读能力枚举 tracked paths。
10. 使用 Git 只读能力枚举未忽略的 untracked paths。
11. 识别并排除 submodule 与嵌套 repository 内部。
12. 对候选路径执行安全校验。
13. 复制或物化所有候选路径至 staging。
14. 创建确定性 manifest。
15. 计算普通文件 SHA-256。
16. 验证 staging 与 manifest 完全一致。
17. 将 staging 内容同步到私有 Fossil checkout。
18. 在 Fossil 中创建 check-in。
19. 取得 Fossil check-in hash。
20. 验证新 check-in 可以重新物化。
21. 清理 staging。
22. 释放锁。
23. 输出 checkpoint ID。

**15.2 并发修改检测**

保存期间，用户、IDE、编译器或其他程序可能修改工作区。

实现 MUST 防止生成内部不一致的 checkpoint。

最低要求：

- 复制每个文件后验证其大小、mtime 或更强标识。
- 对最终复制内容计算 SHA-256。
- 完成扫描后重新验证文件集合未发生影响快照一致性的变化。

如果无法确认整个 snapshot 来自一个自洽的文件世界，save MUST：

- 不发布 checkpoint。
- 删除或隔离临时结果。
- 返回 workspace-changed 错误。
- 建议用户重试。
- 不修改工作区。

这里不承诺操作系统级瞬时快照，但 MUST 避免把明显跨时刻且自相矛盾的内容发布为成功 checkpoint。

**15.3 空 checkpoint**

如果当前 managed universe 与最新 checkpoint 内容完全相同，开放问题如下：

- 仍创建新 checkpoint，以保留时间和消息。
- 无消息时复用最新 checkpoint，有消息时创建新 checkpoint。
- 始终不重复创建。

初步倾向：

> 每次显式 `save` 都创建 checkpoint，即使文件内容相同。

原因：

- 用户动作本身有意义。
- 消息和时间不同。
- 行为最容易预测。
- 去重存储由 Fossil 负责。

**16. `git chkpt list`**

列出当前 worktree 中所有有效 checkpoint。

默认顺序：

```text
创建时间倒序，最新在前。
```

建议输出：

```text
ID          CREATED                         SOURCE        MESSAGE
4f18ac932e  2026-08-16 00:04:21.123 +08:00 manual        parser works
92bcec1a84  2026-08-15 23:58:10.400 +08:00 pre-restore   restore 7d3a882
7d3a882ce1  2026-08-15 23:40:03.003 +08:00 manual
```

规则：

- 时间存储为 UTC。
- 默认展示为本机时区。
- 展示 MUST 包含 UTC 偏移。
- 只列出具有有效 manifest 且完整性可确认的 checkpoint。
- 损坏 checkpoint 不得伪装为正常 checkpoint。
- 是否默认显示损坏项需后续确定。
- 只访问当前 worktree repository。
- 空 repository 返回成功和空列表。

**17. `git chkpt show [CHECKPOINT]`**

显示一个 checkpoint 的元数据和摘要。

省略 ID 时，默认显示最新 checkpoint。

建议输出：

```text
Checkpoint  4f18ac932e7d...
Created     2026-08-16 00:04:21.123 +08:00
Source      manual
Message     parser works before cleanup
Files       138
Bytes       2.4 MiB
```

`show` MAY 显示：

- 文件数量。
- 普通文件总逻辑大小。
- 新增、修改、删除摘要，相对于前一个 checkpoint。
- 完整 hash。
- manifest format version。
- 完整性状态。

`show` MUST NOT：

- 展示 HEAD。
- 展示 branch。
- 解释该 checkpoint 对应哪个 Git commit。
- 声称可以恢复 staged 状态。

是否展示完整文件清单作为默认输出，留待 CLI 设计阶段确定。

**18. `git chkpt diff [CHECKPOINT]`**

比较目标 checkpoint 与当前 managed universe。

省略 ID 时，目标为最新 checkpoint。

语义方向：

```text
target checkpoint → current workspace
```

输出应表达从 checkpoint 变到当前工作区发生了什么。

示例：

```text
M  src/parser.rs
A  tests/parser_cases.rs
D  notes/old-plan.md
```

`diff` SHOULD：

- 对文本文件显示 unified diff。
- 对二进制文件显示 binary changed。
- 表达新增、删除和类型变化。
- 尊重 managed universe。
- 忽略 ignored 文件。
- 不进入 submodule。
- 不修改 Fossil checkout 的持久状态。
- 不修改工作区。

开放问题：

- 是否允许 `git chkpt diff A B`。
- 是否只支持 checkpoint 与当前工作区。
- 二进制文件的大小和 hash 信息展示。
- diff 的退出码是否借鉴常见 diff 工具。

为保持 API 克制，初步倾向只支持：

```bash
git chkpt diff [CHECKPOINT]
```

**19. `git chkpt restore [CHECKPOINT]`**

将当前 managed universe 完整恢复为目标 checkpoint。

省略 ID 时，默认使用最新 checkpoint。

如果最新 checkpoint 与当前工作区相同，restore 仍 MAY 创建 pre-restore checkpoint。为语义稳定，初步倾向始终创建。

成功输出建议：

```text
Saved current workspace as checkpoint e205be92773a
Restored checkpoint 4f18ac932e7d
```

Restore MUST NOT：

- 比较 HEAD。
- 因 branch 不同而警告。
- 因 commit 不同而拒绝。
- 移动 HEAD。
- 修改 index。
- 修改 ref。
- 进入 submodule。
- 询问是否覆盖 managed files。
- 要求 `--force` 才能完整恢复。

**19.1 Restore 算法**

实现 MUST 执行以下步骤：

1. 确认当前目录位于非 bare Git worktree。
2. 解析 worktree 根目录和专属 Git administrative directory。
3. 获取独占锁。
4. 解析目标 checkpoint ID。
5. 校验目标 check-in 存在。
6. 读取并校验 manifest。
7. 在私有 staging 目录中完整物化目标 checkpoint。
8. 验证 staging 与 manifest 完全一致。
9. 对当前 worktree 执行内部 pre-restore save。
10. 验证 pre-restore checkpoint 已成功提交且可重新物化。
11. 创建 restore transaction journal。
12. 扫描当前 managed universe。
13. 计算 restore plan。
14. 准备所有待写文件的同目录临时文件。
15. 创建必要目录。
16. 执行文件创建、替换和类型变化。
17. 删除目标 manifest 中不存在的当前 managed paths。
18. 清理由 restore 产生的空目录。
19. 重新扫描 managed universe。
20. 校验最终工作区与目标 manifest 完全一致。
21. 将 transaction journal 标记为完成。
22. 清理 staging 和临时文件。
23. 释放锁。
24. 输出 pre-restore 和目标 checkpoint ID。

**19.2 Restore plan**

Restore plan 至少包含：

```text
create
replace
delete
type-change
mkdir
rmdir-if-empty
```

每个操作 MUST：

- 使用规范化相对路径。
- 在执行前确认目标仍位于 worktree 根目录。
- 不允许经过 symlink 逃逸到 worktree 外。
- 不接触 `.git`。
- 不接触 ignored 路径。
- 不进入 submodule。
- 在可能时使用同目录临时文件加原子 replace。

**19.3 删除规则**

当前 managed universe 中存在、目标 manifest 中不存在的路径 MUST 删除。

Ignored 路径即使位于将被删除的目录内，也 MUST 保留。

因此不能简单递归删除一个包含 ignored 内容的目录。

例如：

```text
目标 checkpoint:
  src/main.rs

当前工作区:
  src/main.rs
  src/old.rs
  build/cache.bin   ignored
```

恢复后：

```text
src/main.rs         restored
src/old.rs          deleted
build/cache.bin     preserved
```

**19.4 Restore 失败与回滚**

任何工作区修改开始后发生失败，工具 MUST：

1. 将 transaction 标记为 failed 或保留 incomplete 状态。
2. 使用 pre-restore checkpoint 尝试自动回滚。
3. 校验回滚后的 managed universe。
4. 如果回滚成功，报告 restore 失败但原工作区已恢复。
5. 如果回滚失败，保留 journal、staging 和诊断信息。
6. 输出明确的人工恢复指引。
7. 不删除 pre-restore checkpoint。

不得在回滚失败时假装操作已安全结束。

**19.5 进程崩溃后的恢复**

如果下一次命令发现 incomplete restore journal：

- MUST 不执行普通 save、delete 或新的 restore。
- MUST 检查事务阶段。
- SHOULD 自动恢复到 pre-restore checkpoint。
- 如果无法安全判断，MUST 停止并输出确定的人工处理指引。
- MUST NOT 猜测事务已经成功。

此部分需要在后续版本中定义精确状态机。

**20. `git chkpt delete <CHECKPOINT>...`**

删除一个或多个 checkpoint。

示例：

```bash
git chkpt delete 4f18ac9
git chkpt rm 4f18ac9
git chkpt delete 4f18ac9 92bcec1
git chkpt rm 4f18ac9 92bcec1
```

语义：

- delete 是 checkpoint 生命周期中的普通操作。
- 不需要因 checkpoint 未形成 Git commit 而额外警告。
- 不检查 checkpoint 是否“仍有用”。
- 不检查对应内容是否已经进入 Git 历史。
- 不比较 HEAD。
- 不修改工作区。
- 不修改 Git-owned state。

Delete MUST：

1. 获取独占锁。
2. 先解析全部 ID。
3. 任一 ID 不存在或不唯一时，默认不得执行部分删除。
4. 删除对应 checkpoint 的公开可访问性。
5. 保持剩余 Fossil 历史可用。
6. 释放锁。
7. 输出删除结果。

开放问题：

- Fossil check-in 是否能够真正从 repository 物理删除。
- 如果 Fossil 采取不可变历史模型，delete 是否通过隐藏、标记或重建 repository 实现。
- 用户契约是“列表与恢复中不可见”，还是“物理不可恢复”。

初步产品语义应为：

> delete 后，该 checkpoint 不再能通过 `git chkpt` 的公开命令查看或恢复。

是否立即回收物理空间不属于 delete 的必要承诺。

这是 Fossil 后端研究中最重要的未决点之一。

**21. 初始化**

首次执行需要写操作的命令时：

```bash
git chkpt
git chkpt save
git chkpt restore
git chkpt delete
git chkpt rm
```

如果私有目录不存在，实现 MAY 自动初始化。

初始化 MUST：

- 创建私有目录。
- 创建 Fossil repository。
- 关闭 autosync。
- 不配置 remote。
- 创建私有 checkout。
- 写入格式版本。
- 验证 repository 可用。
- 不修改工作区。
- 不修改 Git-owned state。

`list`、`show`、`diff` 在尚未初始化时的行为：

- `list` SHOULD 成功返回空列表。
- `show` SHOULD 返回 no-checkpoint。
- `diff` SHOULD 返回 no-checkpoint。
- 不应仅因只读命令而创建 Fossil repository。

**22. 时间语义**

所有 checkpoint 创建时间 MUST：

- 以 UTC 存储。
- 至少支持微秒精度，如果平台实际提供。
- 使用明确的 `Z` 后缀或等价 UTC 表达。
- 展示时转换为本机时区。
- 展示 UTC 偏移。
- 不依赖本地化日期字符串进行排序或身份判断。

Fossil check-in 自身时间与 manifest 时间不一致时：

- manifest 创建时间是 `git-chkpt` 的规范时间。
- 差异超过允许容差时 SHOULD 报告完整性或外部修改异常。

**23. 完整性校验**

每个 restore 前 MUST 校验：

- Fossil check-in 可读取。
- manifest 存在。
- `format_version` 受支持。
- 所有 manifest 路径合法。
- 所有必要文件存在。
- 文件类型与 manifest 一致。
- 文件大小一致。
- SHA-256 一致。
- 不存在 manifest 未描述的快照内容，内部保留文件除外。
- 不存在逃逸路径。
- 不存在大小写冲突。
- 不存在危险 symlink 解析。

任一校验失败：

- MUST 拒绝 restore。
- MUST NOT 创建 pre-restore checkpoint，除非已经进入写流程。
- MUST NOT 修改工作区。
- MUST 返回 corrupt-checkpoint 或 unsupported-format 错误。

**24. 安全边界**

`git-chkpt` 是本地 checkpoint 工具，不是安全沙箱。

实现仍 MUST 防御：

- 恶意 manifest。
- 路径穿越。
- 绝对路径。
- symlink 逃逸。
- Windows drive path。
- UNC path 注入。
- NUL。
- 大小写折叠冲突。
- TOCTOU 路径替换。
- Fossil repository 被外部篡改。
- 快照内容在 restore 时覆盖 worktree 外文件。

Restore 的任何最终写入目标经过解析后，都 MUST 位于 worktree 根目录内部，并且不得位于 Git administrative directory 内。

**25. 错误模型**

建议的错误分类：

```text
not-a-git-worktree
bare-repository
no-checkpoint
checkpoint-not-found
ambiguous-checkpoint
corrupt-checkpoint
unsupported-format
workspace-changed
unsupported-file-type
repository-busy
fossil-unavailable
fossil-failed
snapshot-failed
pre-restore-failed
restore-failed
rollback-failed
incomplete-transaction
permission-denied
io-error
internal-error
```

每个错误 MUST：

- 输出简洁的人类可读消息。
- 指明操作是否修改了工作区。
- restore 失败时指明回滚是否成功。
- 不输出虚假的成功消息。
- 不泄露不必要的敏感路径。
- 在诊断模式下允许输出底层 Fossil 错误。

退出码映射将在后续版本中确定。

**26. 输出原则**

默认输出应克制、稳定、可读。

成功 save：

```text
Saved checkpoint 4f18ac932e7d
```

成功 restore：

```text
Saved current workspace as checkpoint e205be92773a
Restored checkpoint 4f18ac932e7d
```

失败但回滚成功：

```text
error: failed to restore checkpoint 4f18ac932e7d
Current workspace was restored from pre-restore checkpoint e205be92773a
```

失败且回滚失败：

```text
error: restore failed and automatic rollback did not complete
Pre-restore checkpoint: e205be92773a
Recovery data was preserved at: <path>
```

默认输出不应使用：

- “危险 branch”
- “HEAD 不匹配”
- “checkpoint 来自另一个 commit”
- “是否确定”
- “请使用 --force”

这些概念不属于本工具的状态模型。

**27. 最低验收标准**

实现只有同时通过以下测试，才能被视为满足基础契约。

**27.1 Save 无副作用**

测试前后验证：

- 所有工作区文件内容一致。
- 文件类型一致。
- symlink 一致。
- Git index 文件字节一致。
- HEAD 一致。
- refs 一致。
- `git status --porcelain=v2` 一致。
- 未跟踪文件一致。
- ignored 文件一致。
- Git 操作状态一致。

**27.2 完整恢复**

创建 checkpoint A：

```text
a.txt = A
b.txt = B
```

随后修改为：

```text
a.txt = X
c.txt = C
ignored/cache.bin = CACHE
```

restore A 后必须得到：

```text
a.txt = A
b.txt = B
c.txt 不存在
ignored/cache.bin = CACHE
```

**27.3 Restore 可逆**

1. 保存 A。
2. 修改为 B。
3. restore A。
4. 从 list 中找到自动 pre-restore checkpoint。
5. restore pre-restore checkpoint。
6. 工作区必须完整返回 B。

**27.4 Git 状态独立**

在以下状态分别 save 和 restore：

- 普通 branch。
- detached HEAD。
- staged 修改存在。
- staged 与 unstaged 同时存在。
- merge 进行中。
- rebase 进行中。
- cherry-pick 进行中。

工具不得修改 Git-owned state，也不得因 HEAD 差异拒绝恢复。

是否把 merge/rebase 中测试列为 v1 强制支持，需要实现前再次评估，但状态模型不应因此改变。

**27.5 Worktree 隔离**

1. 创建主 worktree。
2. 创建 linked worktree。
3. 两边分别创建 checkpoint。
4. 两边 `list` 只能看到自己的 checkpoint。
5. 两边 restore 不影响另一 worktree。
6. 两边 Fossil repository 物理独立。

**27.6 `.git` 生命周期**

1. 创建 checkpoint。
2. 删除 `.git`。
3. 重新 `git init`。
4. `git chkpt list` 返回空列表。
5. 工具不得搜索或重新关联旧 checkpoint。

**27.7 Submodule 隔离**

1. 父项目与 submodule 都存在脏文件。
2. 在父项目执行 save。
3. 修改两边内容。
4. 恢复父项目 checkpoint。
5. 父项目内容恢复。
6. submodule 内部内容保持不变。

**27.8 崩溃恢复**

在 restore 各阶段注入崩溃：

- pre-restore 前。
- pre-restore 后。
- staging 后。
- 第一个文件替换后。
- 部分删除后。
- 最终校验前。
- journal 完成前。

下一次运行必须：

- 识别 incomplete transaction。
- 不静默继续普通操作。
- 尽可能恢复 pre-restore 状态。
- 无法自动恢复时保留全部诊断材料。

**28. 当前开放问题**

以下问题需要继续讨论或通过 Fossil 原型验证：

1. Fossil check-in 删除如何实现。
2. delete 的契约是逻辑删除还是物理删除。
3. 空内容重复 checkpoint 是否始终创建。
4. managed universe 的精确 Git 枚举命令。
5. `.gitignore` 在 save 和 restore 期间变化时的确定行为。
6. 当前 ignored 判断应以 restore 开始时还是 pre-restore save 时为准。
7. 空目录是否保存。
8. symlink 在 Windows 上的降级行为。
9. executable bit 的跨平台表达。
10. hard link 是否保留关系。
11. submodule 根路径的 manifest 表达。
12. 嵌套但非 submodule Git repository 的边界识别。
13. 文件在 save 期间变化时的一致性算法。
14. 整树恢复的事务 journal 状态机。
15. restore 删除目录时如何绝对保证 ignored 子内容不丢。
16. Fossil 私有 checkout 是否需要长期保留，还是每次临时创建。
17. Fossil check-in 是否直接包含 manifest 与项目文件，还是包含封装后的 `files/` 目录。
18. 是否需要公开 `--json`。
19. `diff` 是否调用 Fossil diff，还是使用 manifest 物化后自行比较。
20. 超大文件是否设限制。当前原则倾向不设任意限制，但必须处理磁盘空间不足。
21. 文件名编码与不同文件系统兼容问题。
22. 是否支持 bare repository。当前答案为不支持。
23. 是否将 pre-restore checkpoint 与普通 checkpoint 完全等价地展示。
24. 删除最新 checkpoint 后 Fossil 时间线如何继续推进。
25. Fossil repository 损坏后的只读诊断边界。

**29. 当前确定的产品契约摘要**

```text
git chkpt save
    保存当前 worktree 的完整受管理文件世界。
    不改变工作区，不改变 Git。

git chkpt list / git chkpt ls
    列出当前 worktree 的 checkpoint。

git chkpt show
    展示 checkpoint 的说明和摘要。

git chkpt diff
    比较 checkpoint 与当前文件世界。

git chkpt restore
    先自动保存当前世界，再完整恢复目标世界。
    删除目标中不存在的受管理文件。
    不处理 ignored 文件和 .git。

git chkpt delete / git chkpt rm
    删除不再需要的 checkpoint。
```

存储关系：

```text
Git commit
    Git 负责
    正式、长期、共享

Git checkpoint
    Fossil 负责
    临时、本地、per-worktree
```

最核心的一句话：

> **Git 记住已经写进历史的东西，git-chkpt 记住还没来得及写进历史的东西。**

**本稿的写作改进说明**

- **边界**：明确 checkpoint 从属于具体 worktree，而非项目路径或全局应用 registry。
- **状态模型**：彻底移除 HEAD、index、branch 等 Git 内部概念，只保留完整文件世界。
- **恢复语义**：定义为 managed universe 的 100% 恢复，包括删除多余文件。
- **安全性**：用 pre-restore checkpoint、事务 journal 和失败回滚代替确认提示与 `--force`。
- **实现约束**：将 Fossil 确定为唯一后端，不提前设计无用的可插拔抽象。
- **实现可执行性**：主要命令均给出确定步骤，降低实现 LLM 擅自补充语义的空间。
- **待定事项**：将真正需要实验验证的问题独立列出，尤其是 Fossil 删除、并发快照一致性和 restore 事务状态机。
