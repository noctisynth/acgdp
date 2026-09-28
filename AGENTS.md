# acgdp 项目上下文

## 权威文档

- 需求与架构：[docs/prd/acgdp.md](docs/prd/acgdp.md)
- 已知差异与后续事项：[TODO.md](TODO.md)
- 归档处理约束：[.agents/archive-design.md](.agents/archive-design.md)
- 版本与发布流程：[.agents/release.md](.agents/release.md)

PRD 是需求和设计的事实来源。需求或方案改变时，先更新 PRD，再改代码；TODO 只记录 PRD 与当前实现之间的差异。

## 开发约束

- 每个格式都根据内容识别，不能只看扩展名。解压后的文件都要接受同样的检查。
- 最初输入文件不可改动。所有结果先写入临时目录，整条解压链成功后才交付；失败不留下部分输出。
- 拒绝越界路径、符号链接和无提示覆盖。维持嵌套层数与累计解压大小限制。
- 新增或移除依赖、修改包元数据时优先使用 Cargo CLI。生产代码不得对外部输入调用会 panic 的 `unwrap()`。
- 改变用户可见行为、CLI 参数或发布行为时，同步更新 `README.md` 与 `README.CN.md`。
- 修改解压格式、递归、密码或输出行为时，至少运行 `cargo build` 和 `python tests/smoke.py`。

## 每部分完成后的 Git 与 Semifold 流程

1. 更新 PRD/TODO 与相关文档，完成验证。
2. 用 `smif commit` 为该部分创建独立 changeset；运行 `smif status` 核对版本计划。
3. 检查暂存差异，将代码、文档和 changeset 一起提交 Git。

同一分支的后续部分使用新的 changeset，不修改已有 changeset。提交代码不等于执行发布；版本更新和发布由 [发布流程](.agents/release.md) 管理。
