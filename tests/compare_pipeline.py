"""Interleaved end-to-end benchmark for an old and a candidate acgdp binary.

Run: python tests/compare_pipeline.py target/acgdp-baseline.exe target/release/acgdp.exe
"""

import argparse
import io
import random
import shutil
import statistics
import subprocess
import time
import uuid
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIB = 1024 * 1024


def archive(entries):
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_STORED) as writer:
        for name, content in entries:
            writer.writestr(name, content)
    return output.getvalue()


def fixtures(root):
    payload = random.Random(42).randbytes(16 * MIB)
    leaf = archive([("result.bin", payload)])
    single = root / "single.zip"
    single.write_bytes(archive([("inner.jpg", leaf)]))

    multi = root / "multiple.zip"
    multi.write_bytes(
        archive(
            [(f"inner-{index}.jpg", archive([(f"result-{index}.bin", payload)])) for index in range(3)]
            + [("filler.bin", payload * 4)]
        )
    )

    nested = leaf
    for depth in range(3, 0, -1):
        nested = archive([(f"level-{depth}.jpg", nested), (f"filler-{depth}.bin", payload)])
    deep = root / "four.zip"
    deep.write_bytes(nested)
    return {"single": single, "multiple": multi, "four": deep}


def run(binary, source, output_root, index):
    temporary = output_root / f"run-{index}-{uuid.uuid4().hex}"
    temporary.mkdir()
    try:
        output = temporary / "result"
        start = time.perf_counter()
        result = subprocess.run(
            [str(binary), str(source), "--jobs", "2", "-o", str(output)],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        expected = "result.bin" if source.name != "multiple.zip" else "result-0.bin"
        if (output / expected).stat().st_size != 16 * MIB:
            raise RuntimeError(f"wrong output size: {source}")
    finally:
        if not temporary.resolve().is_relative_to(ROOT.resolve()):
            raise RuntimeError(f"temporary path escaped workspace: {temporary}")
        shutil.rmtree(temporary)
    return time.perf_counter() - start


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--rounds", type=int, default=6)
    options = parser.parse_args()
    baseline = options.baseline.resolve(strict=True)
    candidate = options.candidate.resolve(strict=True)
    root = ROOT / "tests" / f".acgdp-compare-{uuid.uuid4().hex}"
    root.mkdir()
    try:
        cases = fixtures(root)
        for name, source in cases.items():
            timings = {"baseline": [], "candidate": []}
            for round_index in range(options.rounds):
                order = ("baseline", "candidate", "candidate", "baseline") if round_index % 2 == 0 else (
                    "candidate", "baseline", "baseline", "candidate"
                )
                for item in order:
                    binary = baseline if item == "baseline" else candidate
                    timings[item].append(run(binary, source, root, len(timings[item])))
            before = statistics.median(timings["baseline"])
            after = statistics.median(timings["candidate"])
            print(
                f"{name}: baseline={before:.3f}s candidate={after:.3f}s "
                f"ratio={after / before:.3f} samples={len(timings['baseline'])} each",
                flush=True,
            )
    finally:
        if not root.resolve().is_relative_to(ROOT.resolve()):
            raise RuntimeError(f"benchmark path escaped workspace: {root}")
        shutil.rmtree(root)


if __name__ == "__main__":
    main()
