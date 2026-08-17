# Documentation Index

`git-chkpt` 现在把不同性质的文档拆开维护，避免把“未来目标”和“当前行为”混在同一个 SPEC 里。

## 文档关系

- `PRODUCT_SPEC_DRAFT.md`
  - 产品目标草案。
  - 包含长期契约、设计倾向、开放问题。
  - 不保证全部已实现。

- `IMPLEMENTATION_SPEC.md`
  - 当前实现规格。
  - 描述代码现在实际承诺和验证的行为。
  - 开发和回归测试应优先对齐此文档。

- `STATUS.md`
  - 当前项目状态。
  - 给不看代码的人快速了解“现在能用到什么程度”。

- `TECHNICAL_NOTES.md`
  - 技术设计说明。
  - 解释 Git / Fossil / manifest / restore journal 等关键设计选择。

- `../README.md`
  - 用户使用说明。
  - 面向安装、命令、常见工作流和限制。

## 命名原则

本项目中：

- `SPEC` 单独出现时，应指当前实现必须满足的规格。
- 还带开放问题或长期目标的文档应显式标记为 `DRAFT`。
- 用户文档不应承诺 implementation spec 尚未实现的能力。
