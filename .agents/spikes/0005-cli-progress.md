# CLI 逐层进度验证

> 日期：2026-09-29；Windows，Rust 1.98.1

`inquire` 的 `RenderConfig::text_input` 控制正在输入的掩码颜色。启用终端颜色时将星号设为浅青色；`--color never` 与 `NO_COLOR` 沿用无色配置。

ZIP 的解压总量来自中央目录各文件的声明大小，7z 的总量来自已解析归档头；实际写出时按字节推进各层进度条。RAR 解压后端没有本实现所需的稳定整体大小与逐字节回调，因而只显示活动指示和已完成条目的写出量。所有进度条只表示本层，不表示递归任务整体比例。非交互输出跳过总量预扫描及动画更新。

`cargo build`、`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`python tests/smoke.py` 与 `cargo test` 通过。构建新版发布二进制后，与旧版按 ABBA/BAAB 顺序交错运行 `python tests/compare_pipeline.py ... --rounds 8`，每场景每版 16 次，输出大小一致：单链 0.111/0.113 秒、多内层包 0.432/0.416 秒、四层链 0.338/0.333 秒（旧/新）。非交互场景未观察到性能回归；终端动态绘制本身不在此基准范围。

交互终端手工验证了中文密码提示、掩码输入、完成输出以及 128 MiB ZIP 产物。另用 2 GiB 合成 ZIP 尝试延长进度条观察时间；该夹具在最终目录重命名时出现 Windows `os error 5`，旧版与新版都复现，因此不将它视作本次进度功能的回归证据。
