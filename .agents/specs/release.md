# acgdp 版本与发布规范

> 状态：Active Spec；发布流程已配置，首次正式发布未完成
>
> 规范基线：2026-09-29
>
> 适用范围：Cargo 包元数据、`.changes/`、`.github/workflows/`、许可证与发行资产
>
> 设计入口：[acgdp 设计索引](../DESIGN.md)
>
> 实施清单：[项目与发布 TODO](../todos/project.md)

## 1. 仓库与许可证

公开 GitHub 仓库为 [`noctisynth/acgdp`](https://github.com/noctisynth/acgdp)，基础分支 `main`，Semifold 发布分支 `release`。项目自身代码采用 Apache-2.0；RAR 后端包含独立许可的 UnRAR C/C++ 代码，源码声明和 [`THIRD_PARTY_LICENSES/UNRAR.txt`](../../THIRD_PARTY_LICENSES/UNRAR.txt) 必须保留。不能把整个二进制的所有组成部分称为仅受 Apache-2.0 约束。

## 2. Changeset 与版本

Semifold 配置位于 [`.changes/config.toml`](../../.changes/config.toml)，包键为 `acgdp`，Rust resolver 使用 `cargo publish`。每完成一个独立部分，创建新的 changeset、运行 `smif status` 核对版本计划，再把实现、规范、清单和 changeset 一起提交 Git。现有 changeset 不改写。

本地 `smif` 版本为 0.2.8；运行前以 `smif --help` 和子命令帮助确认实际 CLI。`smif status` 只是预览，不修改包版本。版本提升、Git 推送、registry 发布和 GitHub Release 分别处理，不因提交 changeset 就宣称已经发布。

## 3. CI 与发布边界

`.github/workflows/semifold-ci.yaml` 在 `main` 推送后运行 Semifold CI，准备发布 PR；发布 PR 合并后才进入 crates.io 与 GitHub Release 流程。`.github/workflows/semifold-status.yaml` 在 PR 上预览状态。不能在本地与 CI 同时手动发布相同版本。

crates.io 发布仍需配置 `CARGO_REGISTRY_TOKEN`，核对 GitHub Actions 权限、首次发布结果和包内容。GitHub Release 已启用，但当前没有多平台预编译二进制附件；Release 记录和可下载的程序是不同交付项。实际发布必须由用户明确授权。

## 4. 用户与工程文档

`README.md` 和 `README.CN.md` 描述构建、用法、限制与许可证；两者的用户可见行为应同步。`.agents/` 存放工程设计、规范和实施清单，不应替代 README。若后续增加稳定的用户指南，可放入 `docs/`，不再把 `.agents` 规范放到 `docs/prd/`。
