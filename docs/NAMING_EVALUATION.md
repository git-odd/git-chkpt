# Naming Evaluation: `git chkpt` vs `git checkpoint`

日期：2026-08-16

## 建议

短期保留主命令 `git chkpt`，产品和文档里继续使用完整概念词 `checkpoint`。

如果后续想加强可发现性，可以额外发布一个极薄的 `git-checkpoint` shim，让 `git checkpoint ...` 委托到同一套实现；但 v0.1 不建议把主命令从 `chkpt` 改名为 `checkpoint`。

## 对比

| 维度 | `git chkpt` | `git checkpoint` | 结论 |
|---|---|---|---|
| 输入效率 | 短，适合频繁敲 | 长，频繁使用更累 | `chkpt` 更适合高频 CLI |
| 语义直观 | 需要 README 解释缩写 | 一眼能懂 | `checkpoint` 更适合第一次接触 |
| Git external command | 对应二进制 `git-chkpt` | 对应二进制 `git-checkpoint` | 两者都符合 Git 外部命令机制 |
| 与项目现状 | crate、binary、目录、文档已统一 | 需要重命名更多资产 | `chkpt` 迁移成本低 |
| 潜在冲突 | 缩写更独特 | 通用词，未来被 Git 或其他工具占用概率更高 | `chkpt` 更稳 |
| 产品心智 | 略像工具名 | 更像功能名 | 文档中用 `checkpoint` 补足 |

## 推荐命名策略

- **命令名**：`git chkpt`
- **包 / 二进制名**：`git-chkpt`
- **概念名**：checkpoint
- **文档表达**：本地 Git checkpoint 工具 / save a checkpoint
- **子命令别名**：保留高频短别名 `ls`、`rm`

这能同时满足：日常输入短、资产命名稳定、用户理解成本低。

## 如果要支持 `git checkpoint`

推荐做成兼容别名，而不是替换：

1. 发布第二个二进制或 shim：`git-checkpoint`。
2. shim 直接转发参数到 `git-chkpt`。
3. README 写成：主命令 `git chkpt`，可选别名 `git checkpoint`。
4. 避免存储目录、manifest、Fossil project name 一起改名，防止破坏现有 checkpoint。

这样可以获得完整词的可发现性，同时不破坏已有 `git chkpt` 用户习惯。
