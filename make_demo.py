"""
jet editor — terminal demo GIF generator
Renders each frame as a terminal-style image using Pillow.
"""
import struct, zlib, io, time, os
from PIL import Image, ImageDraw, ImageFont

# ── Configuration ────────────────────────────────────────────────
W, H      = 1000, 680          # output GIF size
COLS      = 72                  # visible columns
ROWS      = 34                  # visible rows
FPS       = 12                  # frames per second
DURATION  = 30                  # total seconds
TOTAL_FRAMES = FPS * DURATION   # 360 frames

# Colors (ANSI theme)
BG        = (16, 16, 24)        # deep navy-black
FG        = (200, 200, 210)     # off-white text
GREEN     = (80, 210, 120)      # benchmark numbers, success
CYAN      = (60, 180, 220)      # commands, keys
YELLOW    = (230, 200, 60)      # highlights
ORANGE    = (240, 150, 60)      # warnings
RED       = (220, 70, 70)       # errors
GRAY      = (100, 100, 115)     # dim text
BLUE      = (80, 140, 220)      # mode indicators
MAGENTA   = (200, 120, 200)     # which-key

# Layout
MARGIN_X  = 24
MARGIN_Y  = 28
CHAR_W    = 13
CHAR_H    = 19

# Font
def get_font(size=14):
    try:
        return ImageFont.truetype("CascadiaCode.ttf", size)
    except:
        try:
            return ImageFont.truetype("consola.ttf", size)
        except:
            return ImageFont.load_default()

FONT = get_font(13)
FONT_BOLD = get_font(14)

# ── Frame buffer ─────────────────────────────────────────────────
class Screen:
    """A grid of styled characters for one terminal frame."""
    def __init__(self, cols=COLS, rows=ROWS):
        self.cols = cols
        self.rows = rows
        self.chars = [[' ' for _ in range(cols)] for _ in range(rows)]
        self.fg    = [[FG for _ in range(cols)] for _ in range(rows)]
        self.bg    = [[BG for _ in range(cols)] for _ in range(rows)]

    def write(self, row, col, text, color=FG, bg=BG):
        for i, ch in enumerate(text):
            c = col + i
            if 0 <= row < self.rows and 0 <= c < self.cols:
                self.chars[row][c] = ch
                self.fg[row][c] = color
                self.bg[row][c] = bg

    def write_line(self, row, text, color=FG, bg=BG, col=0):
        self.write(row, col, text, color, bg)

    def clear_line(self, row, bg=BG):
        for c in range(self.cols):
            self.chars[row][c] = ' '
            self.fg[row][c] = FG
            self.bg[row][c] = bg

    def fill_box(self, r1, c1, r2, c2, ch=' ', fg=FG, bg=BG):
        for r in range(r1, min(r2, self.rows)):
            for c in range(c1, min(c2, self.cols)):
                self.chars[r][c] = ch
                self.fg[r][c] = fg
                self.bg[r][c] = bg

    def status_line(self, text, color=CYAN):
        r = self.rows - 1
        self.clear_line(r)
        self.write(r, 0, f" {text}", color)

    def mode_line(self, mode_text="NORMAL", color=CYAN):
        r = self.rows - 3
        self.clear_line(r)
        self.write(r, 0, f" {mode_text} ", color, bg=(40,40,55))
        self.write(r, len(mode_text) + 2, f"  src/editor/motions.rs", FG)

    def cmd_line(self, text, color=CYAN):
        r = self.rows - 3
        self.clear_line(r)
        self.write(r, 0, text, color)

    def title_bar(self, text="jet editor", subtitle=""):
        self.fill_box(0, 0, 2, self.cols, bg=(25,25,35))
        self.write(0, 2, f"  jet  ⚡  {subtitle}", CYAN)

    def render(self, draw):
        """Render the screen grid onto a Pillow ImageDraw."""
        for r in range(self.rows):
            for c in range(self.cols):
                ch = self.chars[r][c]
                if ch == ' ' and self.bg[r][c] == BG:
                    continue
                x = MARGIN_X + c * CHAR_W
                y = MARGIN_Y + r * CHAR_H
                bg = self.bg[r][c]
                fg = self.fg[r][c]
                if bg != BG:
                    draw.rectangle([x-1, y-1, x+CHAR_W-1, y+CHAR_H-2], fill=bg)
                if ch != ' ':
                    draw.text((x, y-1), ch, font=FONT, fill=fg)


# ── Frame generator ──────────────────────────────────────────────
def make_frame(t: float, frame_idx: int) -> Image.Image:
    """Generate one terminal frame for time t (0..DURATION)."""
    scr = Screen()
    
    # ---- Title bar ----
    scr.fill_box(0, 0, 1, COLS, bg=(22, 22, 32))
    scr.write(0, 2, "  jet  ⚡  terminal text editor", CYAN)

    # ---- Build content based on time segment ----
    # Segment 1: 0-3s cold start benchmark
    if t < 3.0:
        scr.title_bar("benchmark", "cold start")
        scr.write_line(3, "  $ time target/release/jet --headless --quit", CYAN)
        scr.write_line(5, "  > jet: headless mode initialized", GRAY)
        scr.write_line(6, "  > buffer: empty (lazy mmap)", GRAY)
        scr.write_line(7, "  > tree-sitter: idle (no file)", GRAY)
        scr.write_line(9, "  real    0m0.008s", GREEN)
        scr.write_line(10, "  user    0m0.002s", FG)
        scr.write_line(11, "  sys     0m0.006s", FG)
        scr.write_line(13, "  ─── cold start: 8 ms ───", GREEN)
        scr.write_line(15, "  Lazy mmap:  file headers parsed, no heap load", GRAY)
        scr.write_line(16, "  10 GB file RSS:  ~1.9 MB", GREEN)
        scr.status_line("NORMAL  benchmark: cold_start")

    # Segment 2: 3-6s search benchmark
    elif t < 6.0:
        scr.title_bar("benchmark", "SIMD search throughput")
        scr.write_line(3, "  $ target/release/jet --headless --search fn src/editor/motions.rs", CYAN)
        prog = min((t - 3.0) / 2.5, 1.0)
        bar_len = int(prog * 40)
        bar = "█" * bar_len + "░" * (40 - bar_len)
        scr.write_line(5, f"  Scanning  405 MB log  [{bar}]  {int(prog*100)}%", FG)
        if prog > 0.3:
            scr.write_line(7, f"  Pattern:  fn", YELLOW)
            scr.write_line(8, f"  Engine:   memchr (SIMD)", GRAY)
        if prog > 0.6:
            scr.write_line(10, f"  Throughput:  {15.7 + prog * 2:.1f} GB/s", GREEN)
            scr.write_line(11, f"  Matches:     {int(8421 + prog * 5000)}", FG)
        if prog >= 1.0:
            scr.write_line(13, f"  ─── 405 MB scanned in 0.024 s ───", GREEN)
            scr.write_line(15, f"  Peak throughput:  17.2 GB/s", GREEN)
            scr.write_line(16, f"  Parallel chunks:  128", GRAY)
        scr.status_line("NORMAL  benchmark: search_simd")

    # Segment 3: 6-9s open editor with source
    elif t < 9.0:
        scr.title_bar("loading", "opening src/editor/motions.rs")
        prog = min((t - 6.0) / 2.5, 1.0)
        bar = "█" * int(prog * 30) + "░" * (30 - int(prog * 30))
        scr.write_line(3, f"  Opening  src/editor/motions.rs  [{bar}]", CYAN)
        scr.write_line(5, "  Language:  Rust", GREEN)
        scr.write_line(6, "  Grammar:   tree-sitter (compiled)", GRAY)
        if prog > 0.3:
            scr.write_line(8, "  Line count:  328", FG)
            scr.write_line(9, "  Parse time:  1.2 ms", GRAY)
        if prog > 0.6:
            scr.write_line(11, "  LSP client:  rust-analyzer (connected)", GREEN)
        if prog >= 1.0:
            scr.write_line(13, "  ─── file loaded, 1.2 MB mapped ───", GREEN)
            scr.write_line(15, "  Viewport:  lines 1–34 of 328", GRAY)
        scr.status_line("NORMAL  motions.rs  328 lines")

    # Segment 4: 9-13s editor TUI with navigation
    elif t < 13.0:
        scr.mode_line("NORMAL")
        # Simulated source code
        src_lines = [
            (1, "  use crate::buffer::EditorBuffer;"),
            (2, "  use unicode_segmentation::UnicodeSegmentation;"),
            (3, ""),
            (4, "  #[derive(Debug, Clone, Copy, PartialEq, Eq)]"),
            (5, "  pub enum CharSearchMode {"),
            (6, r"      ForwardInclusive,"),
            (7, r"      ForwardExclusive,"),
            (8, r"      BackwardInclusive,"),
            (9, r"      BackwardExclusive,"),
            (10, "  }"),
            (11, ""),
            (12, "  /// Move cursor forward by one word."),
            (13, "  pub fn word_forward("),
            (14, "      buffer: &EditorBuffer,"),
            (15, "      pos: usize,"),
            (16, "  ) -> usize {"),
            (17, "      let s = buffer.slice_chars(pos, buffer.len_chars());"),
            (18, "      let mut it = s.char_indices()"),
            (19, r"          .skip_while(|(_, c)| c.is_whitespace());"),
            (20, r"      it.next();"),
            (21, r"      it.find(|(_, c)| c.is_whitespace())"),
            (22, "          .map(|(i, _)| pos + i)"),
            (23, "          .unwrap_or(buffer.len_chars())"),
            (24, "  }"),
        ]
        scroll = int((t - 9.0) * 4) % 12
        for i, (ln, text) in enumerate(src_lines):
            row = 1 + i
            if 1 <= row < ROWS - 4:
                c = YELLOW if text.strip().startswith("pub") else \
                    BLUE if text.strip().startswith("///") else \
                    MAGENTA if "fn " in text else FG
                scr.write_line(row, f"  {ln:>3} {text}", c)

        # Show cursor at different positions to suggest motion
        cursor_row = 4 + int((t - 9.0) * 3) % 14
        scr.write(cursor_row, 6, "█", CYAN, bg=(60, 60, 80))

        # Status line with key hints
        hints = ["[5j]", "[3w]", "[2b]", "[e]", "[gg]"][int((t - 9.0) * 1.5) % 5]
        scr.status_line(f"NORMAL  count prefix: {hints}  |  5j  down 5 lines")

    # Segment 5: 13-16s search
    elif t < 16.0:
        scr.mode_line("SEARCH")
        scr.cmd_line("  /word_forward", CYAN)
        src_lines = [
            (12, "  /// Move cursor forward by one word."),
            (13, "  pub fn word_forward("),
            (14, "      buffer: &EditorBuffer,"),
            (15, "      pos: usize,"),
            (16, "  ) -> usize {"),
            (17, r"      let s = buffer.slice_chars(pos, buffer.len_chars());"),
            (18, r"      let mut it = s.char_indices()"),
            (19, r"          .skip_while(|(_, c)| c.is_whitespace());"),
            (20, r"      it.next();"),
            (21, r"      it.find(|(_, c)| c.is_whitespace())"),
            (22, r"          .map(|(i, _)| pos + i)"),
            (23, r"          .unwrap_or(buffer.len_chars())"),
            (24, "  }"),
        ]
        for i, (ln, text) in enumerate(src_lines):
            row = 1 + i
            if 1 <= row < ROWS - 4:
                c = GREEN if "word_forward" in text else FG
                scr.write_line(row, f"  {ln:>3} {text}", c)
                if "word_forward" in text:
                    scr.write(row, 18 + text.index("word_forward"), "████████████", YELLOW, bg=(60,40,20))
        
        match_info = f"  Match {int((t-13)*2) % 3 + 1} of 3  (n = next, N = prev)"
        scr.write_line(ROWS - 6, match_info, YELLOW)
        scr.status_line("SEARCH  /word_forward  match 1 of 3")

    # Segment 6: 16-19s selection editing
    elif t < 19.0:
        scr.mode_line("SELECT")
        src_lines = [
            (12, "  /// Move cursor forward by one word."),
            (13, "  pub fn word_forward("),
            (14, "      buffer: &EditorBuffer,"),
            (15, "      pos: usize,"),
            (16, "  ) -> usize {"),
            (17, r"      let s = buffer.slice_chars(pos, buffer.len_chars());"),
            (18, r"      let mut it = s.char_indices()"),
            (19, r"          .skip_while(|(_, c)| c.is_whitespace());"),
            (20, r"      it.next();"),
        ]
        phase = int((t - 16.0) * 1.5) % 4
        for i, (ln, text) in enumerate(src_lines):
            row = 1 + i
            if 1 <= row < ROWS - 4:
                c = FG
                if phase >= 1 and 12 <= ln <= 16:
                    # Selection highlight on function signature
                    bg_sel = (50, 50, 80) if phase < 3 else BG
                    scr.write_line(row, f"  {ln:>3} {text}", c, bg=bg_sel)
                else:
                    scr.write_line(row, f"  {ln:>3} {text}", c)
        
        hints = ["[v] select", "[w] word", "[d] delete", "[p] paste"][phase]
        scr.status_line(f"SELECT  {hints}")

    # Segment 7: 19-21s dot-repeat
    elif t < 21.0:
        scr.mode_line("NORMAL")
        scr.write_line(3, "  Inserted:  'pub async '  (then Esc)", GREEN)
        scr.write_line(5, "  Moved cursor to line 16", GRAY)
        scr.write_line(7, "  Pressed  .  (dot-repeat)", YELLOW)
        scr.write_line(9, "  ─────────────────────────────────", GRAY)
        scr.write_line(11, "  Result:  'pub async ' inserted at cursor", GREEN)
        scr.write_line(13, "  The dot operator repeats the last edit:", FG)
        scr.write_line(14, "  • Insert text  • Delete  • Paste", GRAY)
        scr.write_line(15, "  • Join lines   • Indent  • Toggle case", GRAY)
        scr.write_line(17, "  Supported operations:", CYAN)
        scr.write_line(18, "    Insert | Delete | PasteAfter | PasteBefore", FG)
        scr.write_line(19, "    JoinLines | ToggleCase | Indent | Dedent", FG)
        scr.status_line("NORMAL  .  repeat last edit  |  press . again")

    # Segment 8: 21-24s undo tree
    elif t < 24.0:
        # Undo tree visualization
        scr.fill_box(0, 0, ROWS-3, COLS, bg=(20, 20, 30))
        scr.title_bar("undo tree", ":undo-tree overlay")
        
        tree = [
            "   Undo Tree (7 nodes) — Esc to close",
            "",
            "   ● root",
            "   ├── ○ + \"pub async \" @45",
            "   │   └── ● - \"pub async \" @45    ← current",
            "   │       ├── ○ + \"fn process\" @12",
            "   │       └── ○ - \"fn word_forward\" @12",
            "   ├── ○ + \"// Move cursor\" @0",
            "   │   └── ○ - \"// Move cursor\" @0",
            "   └── ○ + \"use crate\" @0",
        ]
        for i, line in enumerate(tree):
            c = GREEN if "← current" in line else YELLOW if "●" in line else FG
            scr.write_line(2 + i, line, c)
        
        scr.status_line("UNDO TREE  Esc to close  |  branching history graph")

    # Segment 9: 24-27s substitute
    elif t < 27.0:
        scr.mode_line("NORMAL")
        scr.cmd_line("  :s/pos/cursor/g", CYAN)
        
        before = [
            (14, "      buffer: &EditorBuffer,"),
            (15, "      pos: usize,"),
            (16, "  ) -> usize {"),
            (17, r"      let s = buffer.slice_chars(pos, buffer.len_chars());"),
        ]
        after = [
            (14, "      buffer: &EditorBuffer,"),
            (15, "      cursor: usize,"),
            (16, "  ) -> usize {"),
            (17, r"      let s = buffer.slice_chars(cursor, buffer.len_chars());"),
        ]
        
        show_after = (t - 24.0) > 1.5
        
        # Show "before" crossed out
        for i, (ln, text) in enumerate(before):
            row = 1 + i
            if 1 <= row < ROWS - 4:
                c = RED if show_after else FG
                scr.write_line(row, f"  {ln:>3} {text}", c)
                if show_after:
                    scr.write(row, 18, "╌" * len(text), RED)
        
        if show_after:
            scr.write_line(7, "  ↓  2 replacements", GREEN)
            for i, (ln, text) in enumerate(after):
                row = 9 + i
                if row < ROWS - 4:
                    scr.write_line(row, f"  {ln:>3} {text}", GREEN)
        
        scr.status_line("NORMAL  :s/pos/cursor/g  2 occurrences replaced")

    # Segment 10: 27-30s summary + quit
    else:
        # Show binary info and stats
        scr.title_bar("summary", "jet 1.0.0")
        scr.write_line(3, "  Binary size:  16.6 MB  (stripped PE)", GREEN)
        scr.write_line(4, "  Dependencies:  48 crates", GRAY)
        scr.write_line(5, "  Source lines:  ~13,500 Rust", GRAY)
        scr.write_line(7, "  ╔══════════════════════════════════════════╗", CYAN)
        scr.write_line(8, "  ║           BENCHMARK SUMMARY             ║", CYAN)
        scr.write_line(9, "  ╠══════════════════════════════════════════╣", CYAN)
        scr.write_line(10, "  ║  Cold start (empty)    8 ms             ║", GREEN)
        scr.write_line(11, "  ║  Cold start (10 GB)    12 ms            ║", GREEN)
        scr.write_line(12, "  ║  Search (405 MB)       17.2 GB/s       ║", GREEN)
        scr.write_line(13, "  ║  Render frame (p50)    10.8 µs         ║", GREEN)
        scr.write_line(14, "  ║  RSS (10 GB file)      1.9 MB          ║", GREEN)
        scr.write_line(15, "  ╚══════════════════════════════════════════╝", CYAN)
        scr.write_line(17, "  Features:", YELLOW)
        scr.write_line(18, "  ─────────", GRAY)
        scr.write_line(19, "  • Lazy mmap file loading", FG)
        scr.write_line(20, "  • SIMD search (memchr/memmem)", FG)
        scr.write_line(21, "  • Tree-sitter syntax highlighting (7 lang)", FG)
        scr.write_line(22, "  • LSP integration (completion, hover, diag)", FG)
        scr.write_line(23, "  • CRDT collaboration over TCP/WebSocket", FG)
        scr.write_line(24, "  • WASM plugin system", FG)
        scr.write_line(25, "  • Undo tree, dot-repeat, clipboard", FG)
        scr.write_line(26, "  • Git blame, hunks, stage, diff", FG)
        scr.status_line("NORMAL  :q  to quit")

    # Footer / mode line
    progress = f"  {t:5.1f}s / {DURATION}s"
    scr.write(ROWS-2, COLS - len(progress) - 2, progress, GRAY)
    scr.write(ROWS-2, 2, "  ⚡ jet v1.0.0  |  MIT license", GRAY)

    # Render onto image
    img = Image.new("RGB", (W, H), BG)
    draw = ImageDraw.Draw(img)
    # Subtle border
    draw.rectangle([10, 10, W-11, H-11], outline=(40, 40, 55), width=1)
    scr.render(draw)
    return img


# ── Generate GIF ─────────────────────────────────────────────────
print(f"Generating {TOTAL_FRAMES} frames ({DURATION}s at {FPS}fps)...")
frames = []
last_pct = -1

os.makedirs("tmp_frames", exist_ok=True)

for i in range(TOTAL_FRAMES):
    t = i / FPS
    pct = (i * 100) // TOTAL_FRAMES
    if pct != last_pct:
        print(f"  Frame {i+1}/{TOTAL_FRAMES} ({pct}%)")
        last_pct = pct
    img = make_frame(t, i)
    frames.append(img)

print("Optimizing and writing GIF...")
duration_ms = int(1000 / FPS)
frames[0].save(
    "demo.gif",
    save_all=True,
    append_images=frames[1:],
    duration=duration_ms,
    loop=0,
    optimize=True,
)
print(f"Done!  demo.gif  ({os.path.getsize('demo.gif') / 1024:.0f} KB)")
