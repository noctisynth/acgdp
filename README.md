# acgdp

`acgdp` recursively extracts ZIP, 7z, and RAR archives. It recognizes archives by their contents, including files renamed to `.jpg` or `.png` and valid images with an archive appended. After each extraction, it checks every resulting file for another archive.

The project's own code is licensed under [Apache-2.0](LICENSE). RAR support embeds UnRAR code through `unrar-ng`; its separate terms and restrictions are in the [UnRAR license](THIRD_PARTY_LICENSES/UNRAR.txt).

[简体中文说明](README.CN.md)

## Usage

Rust 1.89 or newer is required to build:

```sh
cargo build --release
```

Run `acgdp` without arguments to display the full command help.

Extract a file:

```sh
acgdp game.jpg --ask-password
```

By default, this creates `game.jpg.extracted` beside the input. The original `game.jpg` stays untouched. On success, nested archive files are removed and their contents are placed in the directories where those archives appeared. Common options:

```sh
acgdp game.jpg -o ./game --ask-password
acgdp game.jpg -p 'shared-password' --keep-intermediates
acgdp game.jpg --max-depth 64 --max-gib 50
acgdp game.jpg --color always
acgdp game.jpg --jobs 1
acgdp game.7z.001 --ask-password
```

`-p` is useful in scripts, but the password may appear in process arguments. Use `--ask-password` for an interactive Chinese prompt that keeps the cursor on the prompt line and hides the input without revealing its length. Unencrypted archives need no password.

Help, extraction progress, and errors use color in a terminal. Progress shows each layer's number, format, and archive name, followed by the output location, total bytes written, and elapsed time. A terminal also shows an activity spinner; redirected output has plain lines without an animated spinner. Set `--color always` to keep colors when redirecting output, or `--color never` to disable them; `auto` is the default.

By default, `--jobs 2` passes completed inner archives to the next worker at every nesting level, allowing multiple layers to overlap. When a 7z member contains a non-solid RAR, the next layer can extract complete RAR entries while that member is still being written. Other format combinations start after the inner file is complete. Use `--jobs 1` for the serial path. `--jobs 2` enables the pipeline and may create a stage at each nested level; it does not cap the process at two threads. The speedup depends on the archive layout and storage device.

## Behavior and limits

- Takes one input file and ignores its extension. ZIP and 7z use Rust libraries; RAR uses `unrar-ng`, which includes RARLAB's UnRAR C/C++ code.
- For a split 7z archive, pass the `.001` file and keep `.002`, `.003`, and later parts beside it. The parts are read as one archive without creating a joined copy.
- Extracts to a temporary directory and creates the final output directory only after every nested layer succeeds. A failure leaves the original input intact and no partial output directory.
- Refuses to overwrite an existing output directory or a file with the same name. It also rejects archive entries with escaping paths or symbolic links.
- Extracts at most 32 layers and writes at most 20 GiB across all layers by default. ZIP and 7z are limited as bytes are written. RAR entry sizes are checked before extraction and the written sizes are checked afterward.
- Multipart RAR archives are not handled specially yet. Encrypted RAR headers and unusual compression methods need more testing with real files.

## Verification

```sh
cargo build
python tests/smoke.py
cargo bench --bench nested_extraction -- --noplot
```

The smoke test covers nested ZIPs, ZIP/7z/RAR appended to images, RAR3/RAR5, encrypted ZIP/7z, and cleanup on failure. The 7z and encrypted ZIP cases require 7-Zip installed locally; the other tests do not. Two RAR fixtures in `tests/fixtures` come from [RAR Test Files](https://github.com/ssokolow/rar-test-files) under CC0. A RAR fixture used to check symbolic link rejection comes from the [libarchive test suite](https://github.com/libarchive/libarchive/blob/master/libarchive/test/test_read_format_rar.rar.uu).

The Criterion benchmark compares `--jobs 1` and `--jobs 2` on generated ZIP fixtures with multiple inner archives, a single chain, and four nested layers. Use `python tests/compare_pipeline.py <baseline> <candidate>` to interleave runs of two binaries. See [benchmark notes](.agents/spikes/0001-pipeline-benchmark.md) for the measured results and limits. A single-run comparison on a real split 7z→RAR5 archive is in the [sample validation](.agents/spikes/0002-real-split-archive.md); results varied between runs, so speedups are not guaranteed.
