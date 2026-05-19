use criterion::{criterion_group, criterion_main, Criterion};
use jet::buffer::rope::EditorBuffer;

fn rust_fixture(lines: usize) -> String {
    let mut text = String::with_capacity(lines * 32);
    for idx in 0..lines {
        text.push_str(&format!("fn f_{idx}() -> usize {{ {idx} }}\n"));
    }
    text
}

fn bench_incremental_reparse(c: &mut Criterion) {
    for lines in [10_000usize, 100_000usize] {
        let text = rust_fixture(lines);
        c.bench_function(&format!("insert_10_chars_{lines}_lines"), |b| {
            b.iter(|| {
                let mut buffer = EditorBuffer::from_text(&text);
                let idx = buffer.len_chars() / 2;
                buffer.insert(idx, "abcdefghij");
                criterion::black_box(buffer.len_chars());
            });
        });
    }
}

criterion_group!(benches, bench_incremental_reparse);
criterion_main!(benches);
