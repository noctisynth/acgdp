# acgdp

`acgdp` recursively extracts ZIP, 7z, and RAR archives. It recognizes archives by their contents, including files renamed to `.jpg` or `.png` and valid images with an archive appended. After each extraction, it checks every resulting file for another archive.

The project's own code is licensed under [Apache-2.0](LICENSE). RAR support embeds UnRAR code through `unrar-ng`; its separate terms and restrictions are in the [UnRAR license](THIRD_PARTY_LICENSES/UNRAR.txt).

[简体中文说明](README.CN.md)

## Usage

Rust 1.89 or newer is required to build:

```sh
cargo build --release
```

Extract a file:

```sh
acgdp game.jpg --ask-password
```

By default, this creates `game.jpg.extracted` beside the input. The original `game.jpg` stays untouched. On success, nested archive files are removed and their contents are placed in the directories where those archives appeared. Common options:

```sh
acgdp game.jpg -o ./game --ask-password
acgdp game.jpg -p 'shared-password' --keep-intermediates
acgdp game.jpg --max-depth 64 --max-gib 50
```

`-p` is useful in scripts, but the password may appear in process arguments. Use `--ask-password` for interactive use. Unencrypted archives need no password.

## Behavior and limits

- Takes one input file and ignores its extension. ZIP and 7z use Rust libraries; RAR uses `unrar-ng`, which includes RARLAB's UnRAR C/C++ code.
- Extracts to a temporary directory and creates the final output directory only after every nested layer succeeds. A failure leaves the original input intact and no partial output directory.
- Refuses to overwrite an existing output directory or a file with the same name. It also rejects archive entries with escaping paths or symbolic links.
- Extracts at most 32 layers and writes at most 20 GiB across all layers by default. ZIP and 7z are limited as bytes are written. RAR entry sizes are checked before extraction and the written sizes are checked afterward.
- Multipart RAR archives are not handled specially yet. Encrypted RAR headers and unusual compression methods need more testing with real files.

## Verification

```sh
cargo build
python tests/smoke.py
```

The smoke test covers nested ZIPs, ZIP/7z/RAR appended to images, RAR3/RAR5, encrypted ZIP/7z, and cleanup on failure. The 7z and encrypted ZIP cases require 7-Zip installed locally; the other tests do not. Two RAR fixtures in `tests/fixtures` come from [RAR Test Files](https://github.com/ssokolow/rar-test-files) under CC0. A RAR fixture used to check symbolic link rejection comes from the [libarchive test suite](https://github.com/libarchive/libarchive/blob/master/libarchive/test/test_read_format_rar.rar.uu).
