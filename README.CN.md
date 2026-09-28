# acgdp

[English](README.md)

`acgdp` 是一个递归解压命令行工具。它根据文件内容识别 ZIP、7z、RAR，包括改成 `.jpg`、`.png` 等后缀的压缩包，以及图片数据后附加压缩包的文件。每解开一层，它会继续检查这一层产出的所有文件，直到找不到新的压缩包。

项目自身采用 [Apache-2.0](LICENSE)。RAR 支持使用 `unrar-ng` 内置的 UnRAR 代码，其许可与使用限制见 [UnRAR 许可证](THIRD_PARTY_LICENSES/UNRAR.txt)。

## 使用

需要 Rust 1.89 或更新版本。构建：

```sh
cargo build --release
```

不带参数运行 `acgdp` 会显示完整命令帮助。

解压文件：

```sh
acgdp game.jpg --ask-password
```

默认在输入文件旁创建 `game.jpg.extracted`。最初的 `game.jpg` 保持不变；成功后，内层压缩包被移除，解出的内容放在内层包原本所在的目录。常用选项：

```sh
acgdp game.jpg -o ./game --ask-password
acgdp game.jpg -p 'shared-password' --keep-intermediates
acgdp game.jpg --max-depth 64 --max-gib 50
```

`-p` 便于脚本使用，但密码可能出现在进程参数中；手动运行建议用 `--ask-password`。如果文件没有加密，不需要提供密码。

## 行为与限制

- 支持单个输入文件；不依赖文件后缀。ZIP 和 7z 使用 Rust 库；RAR 使用 `unrar-ng`，其底层包含 RARLAB 的 UnRAR C/C++ 代码。
- 先解到临时目录，整条嵌套链成功后才生成输出目录。失败时最初的输入文件仍在，且不会留下部分结果。
- 拒绝覆盖现有输出目录或同名文件，也拒绝压缩包中的越界路径与符号链接。
- 默认最多解开 32 层，累计写出最多 20 GiB。ZIP、7z 按实际写出字节限制；RAR 先检查条目声明大小，再核对提取后的大小。
- 当前未针对分卷 RAR 做专门处理。RAR 文件头加密和特殊压缩方式需用实际样本继续验证。

## 验证

```sh
cargo build
python tests/smoke.py
```

冒烟测试包含多层 ZIP、图片附加 ZIP/7z/RAR、RAR3/RAR5、加密 ZIP/7z，以及失败清理。7z 和加密 ZIP 样例需要本机安装 7-Zip；其余测试不依赖 7-Zip。`tests/fixtures` 下两个 RAR 样本来自 [RAR Test Files](https://github.com/ssokolow/rar-test-files)，按 CC0 发布。另一个用于验证符号链接拒绝行为的 RAR 样本来自 [libarchive 测试集](https://github.com/libarchive/libarchive/blob/master/libarchive/test/test_read_format_rar.rar.uu)。
