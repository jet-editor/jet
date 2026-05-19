use criterion::{criterion_group, criterion_main, Criterion};
use jet::ui::renderer::FrameRenderer;

fn bench_render_frame(c: &mut Criterion) {
    let lines: Vec<String> = (0..80)
        .map(|idx| format!("{idx:04} fn main() {{ println!(\"hello\"); }}"))
        .collect();
    let mut renderer = FrameRenderer::new(120, 40);

    c.bench_function("render_frame_p50", |b| {
        b.iter(|| {
            let frame = renderer.render_to_string(lines.iter().map(String::as_str));
            criterion::black_box(frame.len());
        });
    });
}

criterion_group!(benches, bench_render_frame);
criterion_main!(benches);
