use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use jet::{
    buffer::rope::EditorBuffer,
    editor::{search::SearchEngine, word_wrap::wrap_line},
    util::{stats::BufferStats, unicode::display_width},
};

fn ascii_document(lines: usize, line_len: usize) -> String {
    let mut text = String::with_capacity(lines * (line_len + 1));
    for i in 0..lines {
        let prefix = format!("line-{i:08} ");
        text.push_str(&prefix);
        for j in prefix.len()..line_len {
            let ch = b'a' + ((i + j) % 26) as u8;
            text.push(ch as char);
        }
        text.push('\n');
    }
    text
}

fn search_document(size: usize, stride: usize, needle: &str) -> Vec<u8> {
    let mut data = vec![b'a'; size];
    let needle = needle.as_bytes();
    let mut pos = stride;
    while pos + needle.len() < data.len() {
        data[pos..pos + needle.len()].copy_from_slice(needle);
        pos += stride;
    }
    data
}

fn unicode_line(repetitions: usize) -> String {
    "alpha 你 e\u{301} beta\tgamma 日本語 ".repeat(repetitions)
}

fn bench_rope(c: &mut Criterion) {
    let mut group = c.benchmark_group("rope");

    for &(lines, line_len) in &[(1_000, 96), (25_000, 96)] {
        let text = ascii_document(lines, line_len);
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("from_text", text.len()),
            &text,
            |b, text| {
                b.iter(|| EditorBuffer::from_text(black_box(text)));
            },
        );

        let buffer = EditorBuffer::from_text(&text);
        group.bench_with_input(
            BenchmarkId::new("line_lookup", lines),
            &buffer,
            |b, buffer| {
                b.iter(|| {
                    let mut total = 0usize;
                    for line in (0..lines).step_by(97) {
                        total += buffer.line_len(black_box(line));
                    }
                    black_box(total)
                });
            },
        );
    }

    let text = ascii_document(10_000, 96);
    group.throughput(Throughput::Elements(1));
    group.bench_function("middle_insert_remove", |b| {
        b.iter_batched(
            || EditorBuffer::from_text(&text),
            |mut buffer| {
                let mid = buffer.len_chars() / 2;
                buffer.insert(mid, black_box("benchmark"));
                buffer.remove(mid..mid + "benchmark".chars().count());
                black_box(buffer.len_chars())
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("search");
    let needle = "xyzzy";

    for &size in &[1 << 20, 16 << 20, 64 << 20] {
        let data = search_document(size, 64 * 1024, needle);
        let engine = SearchEngine::new(needle);
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::new("sequential_memmem", size),
            &data,
            |b, data| {
                b.iter(|| engine.find_in_buffer(black_box(data)));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("parallel_memmem", size),
            &data,
            |b, data| {
                b.iter(|| engine.find_in_buffer_parallel(black_box(data)));
            },
        );
    }

    let one_byte = SearchEngine::new("z");
    let data = search_document(16 << 20, 128 * 1024, "z");
    group.throughput(Throughput::Bytes(data.len() as u64));
    group.bench_function("single_byte_memchr/16777216", |b| {
        b.iter(|| one_byte.find_in_buffer(black_box(&data)));
    });

    group.finish();
}

fn bench_unicode(c: &mut Criterion) {
    let mut group = c.benchmark_group("unicode");
    let ascii = ascii_document(1, 2_048);
    let unicode = unicode_line(256);

    for (name, text) in [("ascii", &ascii), ("mixed_unicode", &unicode)] {
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(BenchmarkId::new("display_width", name), text, |b, text| {
            b.iter(|| display_width(black_box(text)));
        });

        group.bench_with_input(BenchmarkId::new("wrap_80_cols", name), text, |b, text| {
            b.iter(|| wrap_line(black_box(text), black_box(80)));
        });
    }

    group.finish();
}

fn bench_stats(c: &mut Criterion) {
    let mut group = c.benchmark_group("stats");

    for &(lines, line_len) in &[(1_000, 96), (25_000, 96)] {
        let text = ascii_document(lines, line_len);
        let buffer = EditorBuffer::from_text(&text);
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("buffer_stats", text.len()),
            &buffer,
            |b, buffer| {
                b.iter(|| BufferStats::from_buffer(black_box(buffer)));
            },
        );
    }

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .measurement_time(std::time::Duration::from_secs(5))
        .warm_up_time(std::time::Duration::from_secs(2));
    targets = bench_rope, bench_search, bench_unicode, bench_stats
);
criterion_main!(benches);
