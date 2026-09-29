use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const PAYLOAD_MIB: usize = 16;

fn payload() -> Vec<u8> {
    let mut bytes = vec![0; PAYLOAD_MIB * 1024 * 1024];
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    for (index, byte) in bytes.iter_mut().enumerate() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = (state ^ index as u64) as u8;
    }
    bytes
}

fn make_inner(path: &Path, name: &str, content: &[u8]) {
    let mut archive = ZipWriter::new(File::create(path).expect("create inner fixture"));
    archive
        .start_file(
            name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .expect("start inner entry");
    archive.write_all(content).expect("write inner fixture");
    archive.finish().expect("finish inner fixture");
}

fn make_outer(path: &Path, inners: &[PathBuf], filler: Option<&[u8]>) {
    let mut archive = ZipWriter::new(File::create(path).expect("create outer fixture"));
    for (index, inner) in inners.iter().enumerate() {
        archive
            .start_file(
                format!("layer-{index}.jpg"),
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .expect("start outer entry");
        let mut source = File::open(inner).expect("open inner fixture");
        std::io::copy(&mut source, &mut archive).expect("write outer fixture");
    }
    if let Some(bytes) = filler {
        archive
            .start_file(
                "large-asset.bin",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .expect("start asset entry");
        for _ in 0..4 {
            archive.write_all(bytes).expect("write asset fixture");
        }
    }
    archive.finish().expect("finish outer fixture");
}

fn binary() -> PathBuf {
    std::env::var_os("ACGDP_BENCH_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_acgdp")))
}

fn make_chain_layer(path: &Path, child: &Path, child_name: &str, filler: &[u8]) {
    let mut archive = ZipWriter::new(File::create(path).expect("create chain layer"));
    archive
        .start_file(
            child_name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )
        .expect("start nested archive");
    std::io::copy(
        &mut File::open(child).expect("open nested archive"),
        &mut archive,
    )
    .expect("write nested archive");
    archive
        .start_file(
            format!("filler-{child_name}.bin"),
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )
        .expect("start filler");
    archive.write_all(filler).expect("write filler");
    archive.finish().expect("finish chain layer");
}

fn bench_nested(c: &mut Criterion) {
    let fixtures = tempfile::tempdir().expect("fixture directory");
    let content = payload();
    let inners: Vec<_> = (0..3)
        .map(|index| {
            let path = fixtures.path().join(format!("inner-{index}.zip"));
            make_inner(&path, &format!("result-{index}.bin"), &content);
            path
        })
        .collect();
    let cases = [("multiple_entries", 3), ("single_chain", 1)];
    let mut group = c.benchmark_group("nested_extraction");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(15));
    for (name, entries) in cases {
        let input = fixtures.path().join(format!("{name}.zip"));
        make_outer(
            &input,
            &inners[..entries],
            (entries > 1).then_some(content.as_slice()),
        );
        for jobs in [1, 2] {
            group.bench_with_input(BenchmarkId::new(name, jobs), &jobs, |b, jobs| {
                b.iter(|| {
                    let output = tempfile::tempdir().expect("output directory");
                    let destination = output.path().join("result");
                    let result = Command::new(binary())
                        .arg(&input)
                        .arg("--jobs")
                        .arg(jobs.to_string())
                        .arg("-o")
                        .arg(&destination)
                        .output()
                        .expect("run acgdp");
                    assert!(
                        result.status.success(),
                        "{}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    for index in 0..entries {
                        let extracted = destination.join(format!("result-{index}.bin"));
                        assert_eq!(
                            std::fs::metadata(extracted).expect("result file").len(),
                            content.len() as u64
                        );
                    }
                    if entries > 1 {
                        assert_eq!(
                            std::fs::metadata(destination.join("large-asset.bin"))
                                .expect("large asset")
                                .len(),
                            (content.len() * 4) as u64
                        );
                    }
                });
            });
        }
    }

    let leaf = fixtures.path().join("deep-leaf.zip");
    make_inner(&leaf, "deep-result.bin", &content);
    let third = fixtures.path().join("deep-third.zip");
    make_chain_layer(&third, &leaf, "leaf.jpg", &content);
    let second = fixtures.path().join("deep-second.zip");
    make_chain_layer(&second, &third, "third.png", &content);
    let deep = fixtures.path().join("deep-root.zip");
    make_chain_layer(&deep, &second, "second.dat", &content);
    for jobs in [1, 2] {
        group.bench_with_input(BenchmarkId::new("four_layers", jobs), &jobs, |b, jobs| {
            b.iter(|| {
                let output = tempfile::tempdir().expect("output directory");
                let destination = output.path().join("result");
                let result = Command::new(binary())
                    .arg(&deep)
                    .arg("--jobs")
                    .arg(jobs.to_string())
                    .arg("-o")
                    .arg(&destination)
                    .output()
                    .expect("run acgdp");
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                assert_eq!(
                    std::fs::metadata(destination.join("deep-result.bin"))
                        .expect("result file")
                        .len(),
                    content.len() as u64
                );
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_nested);
criterion_main!(benches);
