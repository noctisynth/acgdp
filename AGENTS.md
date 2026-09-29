# acgdp Agent 协作约定

本文件适用于仓库中的全部任务。工程设计、实施差异和用户文档分别维护，不把临时 TODO 当作需求来源。

## 1. 权威文档

- 设计入口：[.agents/DESIGN.md](.agents/DESIGN.md)
- 递归解压 Active Spec：[.agents/specs/extraction.md](.agents/specs/extraction.md)
- 版本与发布 Active Spec：[.agents/specs/release.md](.agents/specs/release.md)
- 实施清单索引：[.agents/TODO.md](.agents/TODO.md)
- 解压实施清单：[.agents/todos/extraction.md](.agents/todos/extraction.md)
- 项目与发布实施清单：[.agents/todos/project.md](.agents/todos/project.md)
- 架构提案流程：[.agents/rfcs/README.md](.agents/rfcs/README.md)

`.agents/DESIGN.md` 只负责方向、模块边界和文档索引。已确认的行为、安全约束与验收标准以 Active Spec 为技术事实来源。`.agents/TODO.md` 只负责路由清单；`todos/*.md` 只记录 Spec 与当前实现之间的差异。`README.md` 和 `README.CN.md` 面向用户，不能代替工程规范。

## 2. 设计与实现顺序

1. 从设计入口定位受影响的 Active Spec。
2. 跨模块架构变化先更新 `DESIGN.md`；未确认的重大设计决定先写 Draft RFC，经确认后同步 Spec。
3. 已确认的行为变化先更新对应 Spec，再更新实施清单、代码、测试和用户文档。
4. 只有实现和必要验证均完成后，才能勾选实施项。不能用测试或代码中的偶然行为反向定义需求。

Draft RFC 不构成实现依据；不需要 RFC 的局部重构可以直接按现有 Spec 实施。用户指令优先于本文件中的流程约定。

## 3. 解压与安全边界

- 每层按内容识别 ZIP、7z、RAR，不能只看扩展名；图片前缀和伪装后缀必须继续支持。
- 最初输入文件不可改动。结果先写入临时目录，整条链成功后才交付；失败不留下部分输出目录。
- 拒绝越界路径、符号链接和无提示覆盖；保留嵌套层数与累计写出量限制。
- `cli` 只负责参数和终端输出，`detect` 负责内容识别，`extract` 负责各格式解压与共用安全检查，`app` 负责递归与交付。职责变化先同步设计入口和 Spec。
- RAR 后端含另行授权的 UnRAR 代码；分发时保留其许可证文本和源码声明。

## 4. Rust 与验证

- 生产代码不得对外部输入或可恢复错误调用会 panic 的 `unwrap()`；错误应带有可定位的上下文。
- 依赖和包元数据变更优先使用 Cargo CLI；检查实际 manifest 和 lockfile 变化。
- 用户可见行为、CLI 参数或发布行为变化时，同步更新 `README.md` 和 `README.CN.md`。
- 修改识别、递归、密码、安全检查或输出行为时，至少运行 `cargo build`、`cargo fmt --check` 和 `python tests/smoke.py`。仅修改工程文档时检查链接、差异与 Semifold 状态即可。

## 5. 版本与提交

- Semifold 使用 `.changes/config.toml`；基础分支为 `main`，发布分支为 `release`。完成每个独立部分后创建新的 changeset，运行 `smif status`，再将变更与 changeset 一起提交 Git。
- 不改写已有 changeset。Git 提交、版本提升、crates.io 发布和 GitHub Release 是不同操作；发布条件见 [发布 Spec](.agents/specs/release.md)。
- 交付时说明改动、验证结果，以及相对现有设计是否发生变化；未完成事项保留在对应实施清单。
