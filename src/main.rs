use anyhow::{anyhow, Context, Result};
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use jet::{
    app::App, buffer::rope::EditorBuffer, editor::search::SearchEngine, ui::renderer::FrameRenderer,
};
use std::{
    env, fs,
    io::{stdout, Write},
    path::PathBuf,
    time::Instant,
};

#[derive(Parser, Debug, Clone)]
#[command(
    name = "jet",
    version,
    about = "jet - fast terminal text editor",
    long_about = "jet is a terminal text editor with lazy mmap file loading, headless benchmark modes, and SIMD search."
)]
struct Args {
    /// Files to open.
    files: Vec<PathBuf>,

    /// Run without interactive TUI for scripting and benchmarks.
    #[arg(long)]
    headless: bool,

    /// Exit immediately after startup.
    #[arg(long)]
    quit: bool,

    /// Search for PATTERN in the file and print match count.
    #[arg(long, value_name = "PATTERN")]
    search: Option<String>,

    /// Simulate N keystrokes and report latency percentiles.
    #[arg(long, value_name = "ITERATIONS")]
    bench_latency: Option<usize>,

    /// Open file at a specific line number.
    #[arg(short = 'l', long, value_name = "LINE")]
    line: Option<usize>,

    /// Open in read-only mode.
    #[arg(short = 'R', long)]
    read_only: bool,

    /// Override keymap preset for this session.
    #[arg(long, value_name = "PRESET")]
    keymap: Option<String>,

    /// Override theme for this session.
    #[arg(long, value_name = "THEME")]
    theme: Option<String>,

    /// Show file state from N time ago, for future history integrations.
    #[arg(long, value_name = "DURATION")]
    rewind: Option<String>,

    /// Disable LSP client.
    #[arg(long)]
    no_lsp: bool,

    /// Disable syntax highlighting.
    #[arg(long)]
    no_highlight: bool,
}

fn main() -> Result<()> {
    let args = Args::parse_from(normalize_plus_line_args(env::args()));

    if let Some(iterations) = args.bench_latency {
        return run_latency_benchmark(iterations);
    }

    if args.headless {
        return run_headless(&args);
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("create async runtime")?;
    runtime.block_on(run_interactive(args))
}

fn normalize_plus_line_args<I>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut normalized = Vec::new();
    for arg in args {
        if let Some(rest) = arg.strip_prefix('+') {
            if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
                normalized.push("--line".to_string());
                normalized.push(rest.to_string());
                continue;
            }
        }
        normalized.push(arg);
    }
    normalized
}

fn run_headless(args: &Args) -> Result<()> {
    if let Some(pattern) = &args.search {
        let path = args
            .files
            .first()
            .ok_or_else(|| anyhow!("--search requires a file path"))?;
        let file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file) }
            .with_context(|| format!("mmap {}", path.display()))?;
        let engine = SearchEngine::new(pattern);
        let start = Instant::now();
        let count = engine.count_in_bytes(&mmap);
        let elapsed = start.elapsed();
        let seconds = elapsed.as_secs_f64().max(1e-9);
        let gbps = mmap.len() as f64 / seconds / 1_000_000_000.0;
        println!(
            "{} matches found in {:.3}s ({:.2} GB/s)",
            count, seconds, gbps
        );
        return Ok(());
    }

    if args.quit {
        if let Some(path) = args.files.first() {
            let _buffer = EditorBuffer::open(path)?;
        }
        return Ok(());
    }

    for path in &args.files {
        let buffer = EditorBuffer::open(path)?;
        println!(
            "{}: {} bytes, {} visible lines",
            path.display(),
            buffer.len_bytes(),
            buffer.visible_line_count()
        );
    }
    Ok(())
}

fn run_latency_benchmark(iterations: usize) -> Result<()> {
    let iterations = iterations.max(1);
    let mut renderer = FrameRenderer::new(80, 24);
    let mut samples = Vec::with_capacity(iterations);
    let mut text = String::new();

    for i in 0..iterations {
        let start = Instant::now();
        text.push(char::from(b'a' + (i % 26) as u8));
        let lines = [text.as_str()];
        renderer.render_to_string(lines);
        samples.push(start.elapsed().as_micros() as u64);
    }

    samples.sort_unstable();
    let percentile = |pct: f64| -> u64 {
        let idx = ((samples.len() - 1) as f64 * pct).round() as usize;
        samples[idx]
    };

    let min_us = samples[0];
    let p50_us = percentile(0.50);
    let p95_us = percentile(0.95);
    let p99_us = percentile(0.99);
    let max_us = samples[samples.len() - 1];

    println!("Keystroke latency benchmark ({} iterations):", iterations);
    println!("  min:     {}μs", min_us);
    println!("  p50:     {}μs", p50_us);
    println!("  p95:     {}μs", p95_us);
    println!("  p99:     {}μs", p99_us);
    println!("  max:     {}μs", max_us);
    Ok(())
}

async fn run_interactive(args: Args) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let result = {
        let mut app = App::from_args(
            args.files,
            args.line,
            args.read_only,
            args.keymap,
            args.theme,
            !args.no_lsp,
            !args.no_highlight,
        )?;
        app.run()
    };

    let restore = restore_terminal(&mut stdout);
    result.and(restore)
}

fn restore_terminal(stdout: &mut impl Write) -> Result<()> {
    disable_raw_mode()?;
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}
