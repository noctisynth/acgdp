# 项目与发布实施清单

> 状态：Active
>
> 技术事实来源：[版本与发布 Spec](../specs/release.md)
>
> 最后核对：2026-09-29

## 仓库与文档

- [x] 提供英文/中文 README、Apache-2.0 许可证与 UnRAR 独立许可证文本。
- [x] 将 `noctisynth/acgdp` 设为公开仓库并推送 `main`。
- [x] 按设计索引、Active Spec 和实施清单组织 Agent 工程上下文。

## 版本与发行

- [x] 配置 Semifold changeset、Rust resolver、GitHub Release 和 GitHub Actions 工作流。
- [x] 将旧版 Semifold 配置迁移到 0.3.4，并确认线上 CI 能通过配置解析与发布 PR 准备阶段；CI 已通过并创建发布 PR #1。
- [ ] 配置 crates.io 发布令牌，执行首次发布演练并核对 GitHub Actions 权限和结果。
- [ ] 为 GitHub Release 配置、构建并验证多平台预编译二进制资产。
