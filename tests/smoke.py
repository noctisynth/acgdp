"""Local smoke checks for nested and disguised archives; run after cargo build."""

import base64
import binascii
import io
import pathlib
import shutil
import subprocess
import uuid
import zipfile


ROOT = pathlib.Path(__file__).resolve().parents[1]
ACGDP = ROOT / "target" / "debug" / "acgdp.exe"
SEVEN_Z = pathlib.Path(r"C:\Program Files\7-Zip\7z.exe")
PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6"
    "SAAAAABJRU5ErkJggg=="
)
# libarchive's tiny RAR test archive, stored as uuencoded text (BSD license).
# https://github.com/libarchive/libarchive/blob/master/libarchive/test/test_read_format_rar.rar.uu
RAR_UU = """begin 644 test_read_format_rar.rar
M4F%R(1H'`,^0<P``#0````````"$4G0@D#(`%````!0````#0J+(OK=VVCX4
M,`@`I($``'1E<W0N='AT@`BW=MH^MW;:/G1E<W0@=&5X="!D;V-U;65N=`T*
MG2]T()`R``@````(`````WM$R;;13-@^%#`(`/^A``!T97-T;&EN:\`(T4S8
M>E!?VCYT97-T+G1X=,W@=""0.@`4````%`````-"HLB^8W?:/A0P$`"D@0``
M=&5S=&1I<EQT97-T+G1X=,#,8W?:/F-WVCYT97-T('1E>'0@9&]C=6UE;G0-
M"J'(=."0,0````````````,`````8W?:/A0P!P#M00``=&5S=&1I<L#,8W?:
M/F1WVC[FYW3@D#8````````````#`````)VKU3X4,`P`[4$``'1E<W1E;7!T
5>61I<H#,G:O5/L5=VC[$/7L`0`<`
`
end"""


def make_zip(entries):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in entries.items():
            archive.writestr(name, data)
    return stream.getvalue()


def run(*args):
    result = subprocess.run([str(ACGDP), *map(str, args)], capture_output=True, text=True, encoding="utf-8", errors="replace")
    assert result.returncode == 0, result.stdout + result.stderr


for arguments, expected_color in (
    ([], False),
    (["--color", "always", "--help"], True),
    (["--color", "never", "--help"], False),
):
    result = subprocess.run([str(ACGDP), *arguments], capture_output=True)
    assert result.returncode == 0
    assert (b"\x1b[" in result.stdout) == expected_color


base = ROOT / "tests" / f".smoke-{uuid.uuid4().hex}"
base.mkdir()
try:
    inner = make_zip({"result.txt": b"finished"})
    outer = base / "outer.zip"
    outer.write_bytes(make_zip({"inner.jpg": inner}))

    run(outer)
    colored = subprocess.run([str(ACGDP), str(outer), "--color", "always", "-o", str(base / "colored")], capture_output=True)
    assert colored.returncode == 0 and b"\x1b[" in colored.stdout
    plain = subprocess.run([str(ACGDP), str(outer), "--color", "never", "-o", str(base / "plain")], capture_output=True)
    assert plain.returncode == 0 and b"\x1b[" not in plain.stdout
    output = base / "outer.zip.extracted"
    assert outer.exists()
    assert not (output / "inner.jpg").exists()
    assert (output / "result.txt").read_bytes() == b"finished"

    run(outer, "--jobs", "1", "-o", base / "serial")
    assert (base / "serial" / "result.txt").read_bytes() == b"finished"

    siblings = base / "siblings.zip"
    siblings.write_bytes(make_zip({
        "one.jpg": make_zip({"one.txt": b"one"}),
        "two.png": make_zip({"two.txt": b"two"}),
        "plain.txt": b"plain",
        "ordinary.dll": b"binary data" + bytes.fromhex("526172211a070100") + b"not a rar header",
    }))
    run(siblings)
    assert (base / "siblings.zip.extracted" / "one.txt").read_bytes() == b"one"
    assert (base / "siblings.zip.extracted" / "two.txt").read_bytes() == b"two"
    assert (base / "siblings.zip.extracted" / "plain.txt").read_bytes() == b"plain"
    assert (base / "siblings.zip.extracted" / "ordinary.dll").is_file()

    run(outer, "--keep-intermediates", "-o", base / "kept")
    assert (base / "kept" / "inner.jpg").is_file()

    too_deep = subprocess.run([str(ACGDP), str(outer), "--max-depth", "1", "-o", str(base / "shallow")], capture_output=True)
    assert too_deep.returncode != 0 and not (base / "shallow").exists()

    image_zip = base / "photo.png"
    image_zip.write_bytes(PNG + make_zip({"from-image.txt": b"zip payload"}))
    run(image_zip)
    assert (base / "photo.png.extracted" / "from-image.txt").read_bytes() == b"zip payload"

    for name in ("sample-rar3.rar", "sample-rar5.rar"):
        rar_file = base / name
        rar_file.write_bytes((ROOT / "tests" / "fixtures" / name).read_bytes())
        run(rar_file)
        assert (base / f"{name}.extracted" / "testfile.txt").is_file()

    rar_image = base / "rar.png"
    rar_image.write_bytes(PNG + (ROOT / "tests" / "fixtures" / "sample-rar3.rar").read_bytes())
    run(rar_image)
    assert (base / "rar.png.extracted" / "testfile.txt").is_file()

    mixed_rar = base / "mixed-rar.zip"
    mixed_rar.write_bytes(make_zip({"hidden.png": PNG + (ROOT / "tests" / "fixtures" / "sample-rar5.rar").read_bytes()}))
    run(mixed_rar)
    assert (base / "mixed-rar.zip.extracted" / "testfile.txt").is_file()

    symlink_rar = base / "symlink.rar"
    symlink_rar.write_bytes(b"".join(binascii.a2b_uu(line) for line in RAR_UU.splitlines()[1:-1]))
    refused = subprocess.run([str(ACGDP), str(symlink_rar)], capture_output=True)
    assert refused.returncode != 0 and not (base / "symlink.rar.extracted").exists()

    if SEVEN_Z.exists():
        seven_source = base / "seven.txt"
        seven_source.write_bytes(b"7z payload")
        seven_archive = base / "seven.7z"
        subprocess.run([str(SEVEN_Z), "a", str(seven_archive), str(seven_source)], check=True, capture_output=True)
        split_data = seven_archive.read_bytes()
        split_at = len(split_data) // 2
        first_part = base / "split.7z.001"
        second_part = base / "split.7z.002"
        first_part.write_bytes(split_data[:split_at])
        second_part.write_bytes(split_data[split_at:])
        run(first_part)
        assert (base / "split.7z.001.extracted" / "seven.txt").read_bytes() == b"7z payload"
        second_part.unlink()
        missing_part = subprocess.run(
            [str(ACGDP), str(first_part), "-o", str(base / "missing-part")], capture_output=True
        )
        assert missing_part.returncode != 0 and not (base / "missing-part").exists()

        disguised = base / "seven.png"
        disguised.write_bytes(PNG + seven_archive.read_bytes())
        run(disguised)
        assert (base / "seven.png.extracted" / "seven.txt").read_bytes() == b"7z payload"

        mixed_7z = base / "mixed-7z.zip"
        mixed_7z.write_bytes(make_zip({"hidden.jpg": PNG + seven_archive.read_bytes()}))
        run(mixed_7z)
        assert (base / "mixed-7z.zip.extracted" / "seven.txt").read_bytes() == b"7z payload"

        protected_7z = base / "locked.7z"
        subprocess.run(
            [str(SEVEN_Z), "a", str(protected_7z), str(seven_source), "-psecret", "-mhe=on"],
            check=True, capture_output=True,
        )
        run(protected_7z, "-p", "secret")
        assert (base / "locked.7z.extracted" / "seven.txt").read_bytes() == b"7z payload"

        protected_zip = base / "locked.zip"
        subprocess.run(
            [str(SEVEN_Z), "a", "-tzip", str(protected_zip), str(seven_source), "-psecret", "-mem=AES256"],
            check=True, capture_output=True,
        )
        run(protected_zip, "-p", "secret")
        assert (base / "locked.zip.extracted" / "seven.txt").read_bytes() == b"7z payload"

        nested_locked = base / "nested-locked.zip"
        nested_locked.write_bytes(make_zip({"locked.jpg": protected_zip.read_bytes(), "note.txt": b"note"}))
        run(nested_locked, "-p", "secret")
        assert (base / "nested-locked.zip.extracted" / "seven.txt").read_bytes() == b"7z payload"
        nested_failed = subprocess.run(
            [str(ACGDP), str(nested_locked), "-p", "wrong", "-o", str(base / "nested-failed")],
            capture_output=True,
        )
        assert nested_failed.returncode != 0 and not (base / "nested-failed").exists()

        bad = subprocess.run([str(ACGDP), str(protected_zip), "-p", "wrong", "-o", str(base / "failed")], capture_output=True)
        assert bad.returncode != 0
        assert protected_zip.exists() and not (base / "failed").exists()
finally:
    if base.resolve().parent != (ROOT / "tests").resolve() or not base.name.startswith(".smoke-"):
        raise RuntimeError("unsafe smoke cleanup path")
    shutil.rmtree(base)

print("smoke checks passed")
