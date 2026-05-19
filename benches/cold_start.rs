use criterion::{criterion_group, criterion_main, Criterion};
use std::process::Command;
use std::time::{Duration, Instant};

fn bench_cold_start(c: &mut Criterion) {
    let jet_path = env!("CARGO_BIN_EXE_jet");

    c.bench_function("binary_cold_start_empty", |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                let output = Command::new(jet_path)
                    .args(["--headless", "--quit"])
                    .output()
                    .expect("jet binary should run");
                assert!(output.status.success());
            }
            start.elapsed()
        });
    });

    c.bench_function("binary_cold_start_10gb", |b| {
        let dir = std::env::temp_dir().join("jet_bench_10gb");
        let path = dir.join("fixture.bin");
        if !path.exists() {
            std::fs::create_dir_all(&dir).ok();
            let file = std::fs::File::create(&path).expect("create 10GB fixture");
            file.set_len(10_000_000_000).expect("set 10GB sparse");
            file.sync_all().ok();
        }
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                let output = Command::new(jet_path)
                    .args(["--headless", "--quit", &path.to_string_lossy()])
                    .output()
                    .expect("jet binary should open 10GB file");
                assert!(output.status.success());
            }
            start.elapsed()
        });
        std::fs::remove_dir_all(&dir).ok();
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(Duration::from_secs(10))
        .warm_up_time(Duration::from_secs(3));
    targets = bench_cold_start
);
criterion_main!(benches);
