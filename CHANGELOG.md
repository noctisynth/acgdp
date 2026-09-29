# Changelog

## v0.2.0

### Bug Fixes

- [`d285c6b`](https://github.com/noctisynth/acgdp/commit/d285c6bd69e425b451205db629f0e856148119da): Keep the hidden password prompt on one line and present concise, colored extraction progress.
- [`b6af9a9`](https://github.com/noctisynth/acgdp/commit/b6af9a9b7a9389f26fdcc1c6e2c144f2ed5996a4): Migrate Semifold configuration and pin CI to CLI 0.3.4
- [`21d4d26`](https://github.com/noctisynth/acgdp/commit/21d4d261e71f81f2e445eb85e61b9743ef00f23e): Generate release lockfile with registry access in CI
- [`ea2e022`](https://github.com/noctisynth/acgdp/commit/ea2e0221fa2746d982f8adc784a853d44a1f8750): Restore Inquire's styled password prompt with same-line masked input.
- [`f25b9de`](https://github.com/noctisynth/acgdp/commit/f25b9de917b87d68d8a2f10621212298104f4f39): Show command help successfully when no arguments are supplied
- [`465d000`](https://github.com/noctisynth/acgdp/commit/465d000aa832581e64c649b2736eb8e2e4edff02): 使用 inquire 显示 Windows 中文密码提示并隐藏输入，修复本地代码页乱码。

### Chores

- [`545abd8`](https://github.com/noctisynth/acgdp/commit/545abd8d813997f6b45083e51632bf94bb3bc6c2): Reorganize agent context into design index, active specs, RFC workflow, and scoped TODOs
- [`067ae30`](https://github.com/noctisynth/acgdp/commit/067ae30cf50aff30ec89f3de0f39368c45b561ad): 记录单文件字节级流水线的格式限制、候选后端和真实性能验收条件。
- [`288fda3`](https://github.com/noctisynth/acgdp/commit/288fda33800c9c20c0a2917d725457eb41209e97): Link the GitHub repository in package metadata and release notes
- [`52c8823`](https://github.com/noctisynth/acgdp/commit/52c8823a0afcb129074850d4085d64b86b541baa): Make the GitHub repository public and update release documentation
- [`19e4db1`](https://github.com/noctisynth/acgdp/commit/19e4db1e44199ec425f3d5a6ad8b780184513e27): Document project design and configure Semifold GitHub Release
- [`f1d0ae7`](https://github.com/noctisynth/acgdp/commit/f1d0ae7f43dcd9d34db630dbebcd6d71bd42de0e): Follow the latest Semifold 0.3.x release in CI
- [`c32ead8`](https://github.com/noctisynth/acgdp/commit/c32ead8a79d265eb2a02662e707215c0494d14f4): Track byte-stream extraction work beyond completed-entry overlap
- [`3f76290`](https://github.com/noctisynth/acgdp/commit/3f762906a8c88ae17bcd5b92f2801cd9728f23aa): Record verified Semifold 0.3.x CI installation
- [`20db1f2`](https://github.com/noctisynth/acgdp/commit/20db1f24c4b6aa86e239e1fe6071ccaddab7e0e8): Record successful Semifold CI release preparation

### New Features

- [`3295e3b`](https://github.com/noctisynth/acgdp/commit/3295e3b52c9f6845d080537c8fbadf4405e35fcf): 在 7z 写出期间解压非 solid RAR，支持图片前缀与加密文件头，并在真实分卷样本验证输出一致性。
- [`4e52474`](https://github.com/noctisynth/acgdp/commit/4e52474260852e83fa8505fe611bdaa8be3bec00): Add colored help, extraction progress, and errors with configurable color mode
- [`1b316b8`](https://github.com/noctisynth/acgdp/commit/1b316b8a932fe0ab7ecd5ae6932d7485b3fab03a): Color the interactive password mask and show per-layer byte progress for ZIP and 7z archives.
- [`eae7422`](https://github.com/noctisynth/acgdp/commit/eae742234d7c305996908c64d6497af299349a19): Add recursive ZIP, 7z, and RAR extraction with disguised archive detection.
- [`41075b2`](https://github.com/noctisynth/acgdp/commit/41075b244dc1dd7a548a10d68a3580cea7424b7a): 将跨层解压流水线递归应用于连续多层嵌套，并用交错基准检查改动前后性能。
- [`61f4793`](https://github.com/noctisynth/acgdp/commit/61f4793b37864cc40b6321cee402e66f110c33a2): 支持直接读取连续的 7z .001 分卷，并校验嵌入式归档签名以避免普通文件误判。

### Performance Improvements

- [`5d780b6`](https://github.com/noctisynth/acgdp/commit/5d780b65986d287b0700e8fa459f4bbaa3e439a6): Overlap completed nested archives with parent extraction and benchmark against serial mode
- [`5f474d2`](https://github.com/noctisynth/acgdp/commit/5f474d2d712f9110f0c7138950993a7db0832586): 复用小型 RAR 条目的解码数据进行嵌套格式识别，并优化大文件扫描策略。

### Refactors

- [`5b4ee4b`](https://github.com/noctisynth/acgdp/commit/5b4ee4bdcae8ddaa57edfb1ea7f7d38cb52c7990): Split CLI, detection, archive backends, and recursive orchestration into modules
