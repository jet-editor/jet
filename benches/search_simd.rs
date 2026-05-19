use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use jet::editor::search::SearchEngine;
use std::io::{Seek, Write};

fn make_search_fixture(size: usize) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().expect("temp file");
    // Write first 4KB with pattern, rest sparse via seek + write at end
    let chunk_size = size.min(4_096);
    let mut chunk = Vec::with_capacity(chunk_size);
    let needle = b"ERROR";
    for idx in (0..chunk_size).step_by(4096) {
        let end = (idx + needle.len()).min(chunk_size);
        chunk.resize(end, b'a');
        chunk[idx..end].copy_from_slice(&needle[..end - idx]);
    }
    file.write_all(&chunk).expect("write fixture head");
    if size > chunk_size {
        let f = file.as_file_mut();
        f.set_len(size as u64).expect("set sparse file");
        f.seek(std::io::SeekFrom::End(-(needle.len() as i64)))
            .expect("seek to end");
        f.write_all(needle).expect("write tail pattern");
    }
    file.flush().expect("flush fixture");
    file
}

fn bench_search_simd(c: &mut Criterion) {
    let engine = SearchEngine::new("ERROR");
    let mut group = c.benchmark_group("search_simd");

    // 64MB case (fast, kept from original)
    {
        let size = 64 * 1024 * 1024;
        let mut data = vec![b'a'; size];
        for idx in (0..size).step_by(4096) {
            data[idx..idx + 5].copy_from_slice(b"ERROR");
        }
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function("count_error_64mb", |b| {
            b.iter(|| criterion::black_box(engine.count_in_bytes(&data)));
        });
    }

    // 405MB case (matches README claim)
    {
        let file = make_search_fixture(405_000_000);
        let mmap = unsafe { memmap2::Mmap::map(&file).expect("mmap 405MB fixture") };
        group.throughput(Throughput::Bytes(405_000_000));
        group.bench_function("count_error_405mb", |b| {
            b.iter(|| criterion::black_box(engine.count_in_bytes(&mmap)));
        });
    }

    // 2GB case
    {
        let file = make_search_fixture(2_000_000_000);
        let mmap = unsafe { memmap2::Mmap::map(&file).expect("mmap 2GB fixture") };
        group.throughput(Throughput::Bytes(2_000_000_000));
        group.bench_function("count_error_2gb", |b| {
            b.iter(|| criterion::black_box(engine.count_in_bytes(&mmap)));
        });
    }

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(std::time::Duration::from_secs(10))
        .warm_up_time(std::time::Duration::from_secs(3));
    targets = bench_search_simd
);
criterion_main!(benches);
