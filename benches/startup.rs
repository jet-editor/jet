use criterion::{criterion_group, criterion_main, Criterion};
use jet::buffer::rope::EditorBuffer;

fn bench_startup(c: &mut Criterion) {
    c.bench_function("empty_buffer_startup", |b| {
        b.iter(|| {
            let buffer = EditorBuffer::new();
            criterion::black_box(buffer.len_bytes());
        });
    });
}

criterion_group!(benches, bench_startup);
criterion_main!(benches);
