# 版本与发布

`acgdp` 使用 Semifold 管理 changeset、版本和发布。配置位于 [`.changes/config.toml`](../.changes/config.toml)，包键为 `acgdp`。基础分支是 `main`，发布分支是 `release`；两者必须不同。

每完成一部分，创建新 changeset、运行 `smif status`、提交代码和 changeset。`smif status` 只预览计划，不修改版本或发布。当前工作站安装的 `smif` 为 0.2.8，运行前以 `smif --help` 与子命令 `--help` 为准。

生成的 GitHub Actions 工作流位于 `.github/workflows`：推送 `main` 时由 Semifold 准备发布 PR，发布 PR 合并后执行 crates.io 与 GitHub Release 发布。不要同时在本地手动执行相同版本的 `smif version` 或 `smif publish`。

GitHub 远端是 `noctisynth/acgdp`，当前仓库为私有。公开发布前还需要确定仓库可见性、配置 `CARGO_REGISTRY_TOKEN`，并核对 GitHub Actions 权限与发布结果。仓库自身使用 Apache-2.0；RAR 后端包含另行授权的 UnRAR 代码，发布包应附带 [`THIRD_PARTY_LICENSES/UNRAR.txt`](../THIRD_PARTY_LICENSES/UNRAR.txt)。当前工作流未配置预编译二进制资产；GitHub Release 的版本记录与可下载的多平台程序是两项不同工作。
