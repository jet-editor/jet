use criterion::{criterion_group, criterion_main, Criterion};
use jet::buffer::rope::EditorBuffer;
use std::io::Write;
use tempfile::NamedTempFile;

fn bench_open_large_file(c: &mut Criterion) {
    let mut file = NamedTempFile::new().expect("temp file");
    let chunk = vec![b'a'; 1024 * 1024];
    for _ in 0..16 {
        file.write_all(&chunk).expect("write fixture");
    }
    file.flush().expect("flush fixture");
    let path = file.path().to_path_buf();

    c.bench_function("open_lazy_16mb", |b| {
        b.iter(|| {
            let buffer = EditorBuffer::open(&path).expect("open lazy mmap");
            criterion::black_box(buffer.len_bytes());
        });
    });
}

criterion_group!(benches, bench_open_large_file);
criterion_main!(benches);
