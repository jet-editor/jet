#[cfg(target_os = "linux")]
mod linux_impl {
    use criterion::Criterion;
    use std::process::Command;
    use std::time::Duration;

    fn measure_rss_mb(path: &str) -> u64 {
        let file_path = std::path::PathBuf::from(path);
        let child = Command::new(env!("CARGO_BIN_EXE_jet"))
            .args(["--headless", &file_path.to_string_lossy()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("jet binary should spawn");
        std::thread::sleep(Duration::from_millis(200));
        let pid = child.id() as i32;
        let rss_kb = procfs::process::Process::new(pid)
            .ok()
            .and_then(|p| p.stat().ok())
            .map(|stat| stat.rss_bytes() as u64 / 1024)
            .unwrap_or(0);
        let _ = Command::new("kill").arg(pid.to_string()).status();
        rss_kb / 1024
    }

    pub fn bench_rss(c: &mut Criterion) {
        let mut group = c.benchmark_group("rss");
        for (name, size) in [
            ("10mb", 10_000_000u64),
            ("100mb", 100_000_000),
            ("1gb", 1_000_000_000),
        ] {
            let dir = std::env::temp_dir().join("jet_bench_rss");
            let path = dir.join(format!("{name}.bin"));
            if !path.exists() {
                std::fs::create_dir_all(&dir).ok();
                let file = std::fs::File::create(&path).expect("create RSS fixture");
                file.set_len(size).expect("set sparse file");
                file.sync_all().ok();
            }
            group.bench_function(&format!("rss_{name}"), |b| {
                b.iter_custom(|_iters| {
                    let rss = measure_rss_mb(&path.to_string_lossy());
                    std::time::Duration::from_nanos(rss)
                });
            });
        }
        let dir = std::env::temp_dir().join("jet_bench_rss");
        let path = dir.join("10gb.bin");
        if !path.exists() {
            std::fs::create_dir_all(&dir).ok();
            let file = std::fs::File::create(&path).expect("create 10GB fixture");
            file.set_len(10_000_000_000).expect("set 10GB sparse");
            file.sync_all().ok();
        }
        group.bench_function("rss_10gb", |b| {
            b.iter_custom(|_iters| {
                let rss = measure_rss_mb(&path.to_string_lossy());
                std::time::Duration::from_nanos(rss)
            });
        });
        group.finish();
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(not(target_os = "linux"))]
mod linux_impl {
    pub fn bench_rss(_c: &mut criterion::Criterion) {}
}

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_rss_wrapper(c: &mut Criterion) {
    linux_impl::bench_rss(c);
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(std::time::Duration::from_secs(5))
        .warm_up_time(std::time::Duration::from_secs(2));
    targets = bench_rss_wrapper
);
criterion_main!(benches);
