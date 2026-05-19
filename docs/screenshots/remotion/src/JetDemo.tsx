import { AbsoluteFill, useCurrentFrame, interpolate, spring, Easing, useVideoConfig } from "remotion";

const FPS = 30;
const SCENE_DURATION = FPS * 6;
const SCENE_NAMES = ["Benchmarks", "Search", "Render", "Editor", "Summary"];

const FONT = "'JetBrains Mono', 'Cascadia Code', 'Fira Code', 'Consolas', monospace";
const UI_FONT = "-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif";

const C = {
  bg: "#0b0f14",
  surface: "#131920",
  border: "#1e2630",
  text: "#e8edf5",
  dim: "#6e7a8a",
  accent: "#58a6ff",
  green: "#3fb950",
  red: "#f85149",
  purple: "#bc8cff",
  orange: "#d29922",
  blue: "#79c0ff",
  cyan: "#56d4dd",
  pink: "#f778ba",
  titleBg: "#161d27",
  cursor: "rgba(88,166,255,0.85)",
};

type Line = { text: string; color: keyof typeof C; delay?: number };

const SCENES: { lines: Line[] }[] = [
  // Scene 0 — Benchmarks
  {
    lines: [
      { text: " jet editor  v1.0.0", color: "accent" },
      { text: "", color: "dim" },
      { text: "  Starting benchmarks...", color: "dim", delay: 6 },
      { text: "", color: "dim" },
      { text: "  binary_cold_start_empty", color: "text", delay: 14 },
      { text: "    time:   [16.93 ms  16.96 ms  16.99 ms]", color: "green", delay: 20 },
      { text: "", color: "text" },
      { text: "  binary_cold_start_10gb", color: "text", delay: 28 },
      { text: "    time:   [6.43 s  6.52 s  6.61 s]", color: "green", delay: 34 },
      { text: "", color: "text" },
      { text: "  ✓ Cold start: ~17 ms", color: "green", delay: 42 },
      { text: "  ✓ 10 GB file: ~6.5 s (lazy mmap)", color: "green", delay: 46 },
    ],
  },
  // Scene 1 — Search
  {
    lines: [
      { text: " SIMD Search — memchr / memmem", color: "accent" },
      { text: "", color: "dim" },
      { text: "  sequential_memmem / 1 MiB", color: "text", delay: 6 },
      { text: "    throughput:  40.5 GiB/s", color: "blue", delay: 12 },
      { text: "", color: "text" },
      { text: "  parallel_memmem / 1 MiB", color: "text", delay: 18 },
      { text: "    throughput:  39.4 GiB/s", color: "blue", delay: 24 },
      { text: "", color: "text" },
      { text: "  single_byte_memchr / 16 MiB", color: "text", delay: 30 },
      { text: "    throughput:  24.6 GiB/s", color: "blue", delay: 36 },
      { text: "", color: "text" },
      { text: "  ✓ Peak: 40.5 GiB/s sequential", color: "green", delay: 42 },
      { text: "  ✓ 24.6 GiB/s single-byte (memchr)", color: "green", delay: 46 },
    ],
  },
  // Scene 2 — Render
  {
    lines: [
      { text: " Render Frame — optimization results", color: "accent" },
      { text: "", color: "dim" },
      { text: "  render_frame_p50 (before)", color: "dim", delay: 6 },
      { text: "    time:  10.2 µs", color: "red", delay: 10 },
      { text: "", color: "text" },
      { text: "  render_frame_p50 (after)", color: "text", delay: 16 },
      { text: "    time:  3.68 µs", color: "green", delay: 20 },
      { text: "    change:  -64%", color: "green", delay: 24 },
      { text: "", color: "text" },
      { text: "  open_lazy_16mb", color: "text", delay: 30 },
      { text: "    time:  9.42 ms", color: "green", delay: 34 },
      { text: "    change:  -16.8%", color: "green", delay: 38 },
      { text: "", color: "text" },
      { text: "  ✓ 64% faster rendering", color: "green", delay: 44 },
      { text: "  ✓ 17% faster large-file open", color: "green", delay: 48 },
    ],
  },
  // Scene 3 — Editor
  {
    lines: [
      { text: "  ┌─ jet/src/main.rs ──────────────────────┐", color: "dim" },
      { text: "  │  1  fn main() {                        │", color: "purple", delay: 6 },
      { text: "  │  2      let path = \"large_file.log\";   │", color: "text", delay: 8 },
      { text: "  │  3      let file = File::open(path);   │", color: "text", delay: 10 },
      { text: "  │  4      let reader = BufReader::new(   │", color: "text", delay: 12 },
      { text: "  │  5          file.unwrap()               │", color: "text", delay: 14 },
      { text: "  │  6      );                              │", color: "text", delay: 16 },
      { text: "  │  7      let mut count = 0usize;         │", color: "text", delay: 18 },
      { text: "  │  8      for line in reader.lines() {   │", color: "purple", delay: 20 },
      { text: "  │  9          count += 1;                 │", color: "text", delay: 22 },
      { text: "  │ 10      }                              │", color: "text", delay: 24 },
      { text: "  │ 11      println!(\"{}\", count);        │", color: "text", delay: 26 },
      { text: "  │ 12  }                                  │", color: "text", delay: 28 },
      { text: "  └────────────────────────────────────────┘", color: "dim", delay: 30 },
      { text: "", color: "text" },
      { text: "  ✓ Syntax highlighting", color: "green", delay: 36 },
      { text: "  ✓ Cursorline / Search hl", color: "green", delay: 40 },
      { text: "  ✓ Bracket matching / Line numbers", color: "green", delay: 44 },
    ],
  },
  // Scene 4 — Summary
  {
    lines: [
      { text: "", color: "text" },
      { text: "", color: "text" },
      { text: "", color: "text" },
      { text: "", color: "text" },
      { text: "", color: "text" },
      { text: "", color: "text" },
      { text: "            ╔══════════════════════════════╗", color: "accent", delay: 6 },
      { text: "            ║                              ║", color: "accent", delay: 8 },
      { text: "            ║    j e t   e d i t o r       ║", color: "accent", delay: 10 },
      { text: "            ║                              ║", color: "accent", delay: 12 },
      { text: "            ║  Cold start   ~17 ms         ║", color: "accent", delay: 16 },
      { text: "            ║  Search       ~40 GiB/s      ║", color: "accent", delay: 20 },
      { text: "            ║  Render       ~3.7 µs/frame  ║", color: "accent", delay: 24 },
      { text: "            ║  File open    ~9 ms (16 MB)  ║", color: "accent", delay: 28 },
      { text: "            ║                              ║", color: "accent", delay: 30 },
      { text: "            ║  LSP  ·  CRDT  ·  Plugins    ║", color: "accent", delay: 34 },
      { text: "            ║  Treesitter · Git · Ragged   ║", color: "accent", delay: 38 },
      { text: "            ║  SIMD search · Terminal      ║", color: "accent", delay: 42 },
      { text: "            ║                              ║", color: "accent", delay: 44 },
      { text: "            ╚══════════════════════════════╝", color: "accent", delay: 46 },
    ],
  },
];

function typeOn(
  frame: number,
  start: number,
  speed: number,
  totalChars: number
) {
  if (frame < start) return 0;
  return Math.min(1, ((frame - start) * speed) / totalChars);
}

function TypedLine({
  text,
  color,
  delay = 0,
  frame,
  base,
  lineIdx,
  sceneIdx,
}: {
  text: string;
  color: keyof typeof C;
  delay: number;
  frame: number;
  base: number;
  lineIdx: number;
  sceneIdx: number;
}) {
  const f = frame - base;
  const appear = delay * 1.2;
  if (f < appear) return null;

  const opacity = Math.min(1, (f - appear) / 8);
  const y = (1 - Math.min(1, (f - appear) / 10)) * 14;

  const isCursorLine = sceneIdx === 3 && lineIdx === 9;
  const isSearchHl = sceneIdx === 3 && text.includes("count") && lineIdx > 9;

  return (
    <div
      style={{
        height: 27,
        lineHeight: "27px",
        fontFamily: FONT,
        fontSize: 15,
        color: C[color],
        opacity,
        transform: `translateY(${y}px)`,
        whiteSpace: "pre",
        paddingLeft: 8,
        backgroundColor: isCursorLine
          ? "rgba(88,166,255,0.07)"
          : isSearchHl
            ? "rgba(187,128,9,0.2)"
            : "transparent",
        borderRadius: 2,
      }}
    >
      {text.includes("│") && lineIdx !== 0 && lineIdx !== 15 ? (
        <>
          <span style={{ color: C.dim }}>{text.slice(0, 2)}</span>
          <span>{text.slice(2, 6)}</span>
          <span style={{ color: C.text }}>
            {highlightSyntax(text.slice(6, -2))}
          </span>
          <span style={{ color: C.dim }}>{text.slice(-2)}</span>
        </>
      ) : sceneIdx === 0 && text.includes("time:") ? (
        <>
          <span>{text.split("[")[0]}</span>
          <span style={{ color: C.green }}>
            {"[" + text.split("[")[1]}
          </span>
        </>
      ) : (
        text
      )}
    </div>
  );
}

function highlightSyntax(code: string): React.ReactNode {
  const tokens: { text: string; color: string }[] = [];
  const re = /\b(fn|let|mut|for|in|use|impl|struct|enum|match|if|else|while|return|Ok|async|await|pub)\b|"(?:[^"\\]|\\.)*"|'[^']*'|\b\d+(usize|u64|i32)?\b|!|\(|\)|\{|\}|\[|\]|\.|,/g;
  let last = 0;
  let m: RegExpExecArray | null;
  const r = new RegExp(re.source, re.flags);
  while ((m = r.exec(code)) !== null) {
    if (m.index > last) {
      tokens.push({ text: code.slice(last, m.index), color: C.text });
    }
    const t = m[0];
    if (t.startsWith('"') || t.startsWith("'")) {
      tokens.push({ text: t, color: C.orange });
    } else if (/^\d/.test(t)) {
      tokens.push({ text: t, color: C.blue });
    } else if (t === "!" || t === "." || t === ",") {
      tokens.push({ text: t, color: C.dim });
    } else if (/^[(){}\[\]]$/.test(t)) {
      tokens.push({ text: t, color: C.text });
    } else {
      tokens.push({ text: t, color: C.purple });
    }
    last = m.index + t.length;
  }
  if (last < code.length) {
    tokens.push({ text: code.slice(last), color: C.text });
  }
  return tokens.map((t, i) => (
    <span key={i} style={{ color: t.color }}>
      {t.text}
    </span>
  ));
}

function ProgressBar({
  frame,
  start,
  value,
  max,
  color,
  label,
}: {
  frame: number;
  start: number;
  value: number;
  max: number;
  color: keyof typeof C;
  label: string;
}) {
  const f = frame - start;
  if (f < 0) return null;
  const outerOpacity = Math.min(1, f / 8);
  const pct = Math.min(1, value / max);
  const width = interpolate(pct, [0, 1], [0, 240], {
    easing: Easing.out(Easing.cubic),
  });

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        opacity: outerOpacity,
        paddingLeft: 8,
        height: 24,
      }}
    >
      <span
        style={{
          fontFamily: FONT,
          fontSize: 13,
          color: C.dim,
          width: 140,
          textAlign: "right",
        }}
      >
        {label}
      </span>
      <div
        style={{
          width: 240,
          height: 8,
          backgroundColor: "rgba(255,255,255,0.06)",
          borderRadius: 4,
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width,
            height: "100%",
            backgroundColor: C[color],
            borderRadius: 4,
            boxShadow: `0 0 8px ${C[color]}40`,
            transition: "none",
          }}
        />
      </div>
    </div>
  );
}

function SceneLines({
  lines,
  frame,
  base,
  sceneIdx,
}: {
  lines: Line[];
  frame: number;
  base: number;
  sceneIdx: number;
}) {
  return (
    <>
      {lines.map((line, i) => (
        <TypedLine
          key={i}
          {...line}
          frame={frame}
          base={base}
          lineIdx={i}
          sceneIdx={sceneIdx}
        />
      ))}
      {sceneIdx === 0 && frame > base + 20 && frame < base + 90 && (
        <ProgressBar
          frame={frame}
          start={base + 30}
          value={frame - base - 30}
          max={40}
          color="green"
          label="cold_start_empty"
        />
      )}
      {sceneIdx === 0 && frame > base + 55 && frame < base + 120 && (
        <ProgressBar
          frame={frame}
          start={base + 70}
          value={frame - base - 70}
          max={40}
          color="accent"
          label="cold_start_10gb"
        />
      )}
      {sceneIdx > 0 && sceneIdx < 4 && (
        <div
          style={{
            position: "absolute",
            bottom: 16,
            left: 24,
            fontFamily: FONT,
            fontSize: 12,
            color: C.dim,
          }}
        >
          {"▌" + SCENE_NAMES[sceneIdx]}
        </div>
      )}
    </>
  );
}

function TerminalFrame({
  children,
  title,
  frame,
  sceneIdx,
}: {
  children: React.ReactNode;
  title: string;
  frame: number;
  sceneIdx: number;
}) {
  const entrance = Math.min(1, frame / 12);
  const slideOut = sceneIdx > 0 && frame % SCENE_DURATION > SCENE_DURATION - 8;
  const opacity = slideOut ? Math.max(0, (SCENE_DURATION - (frame % SCENE_DURATION)) / 8) : 1;

  const tintColors: Record<number, string> = {
    0: "rgba(88,166,255,0.03)",
    1: "rgba(188,140,255,0.03)",
    2: "rgba(63,185,80,0.03)",
    3: "rgba(88,166,255,0.03)",
    4: "rgba(188,140,255,0.05)",
  };

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        opacity: opacity * entrance,
        transform: `scale(${0.96 + 0.04 * entrance})`,
      }}
    >
      <div
        style={{
          width: 1060,
          height: sceneIdx === 4 ? 620 : 660,
          borderRadius: 14,
          overflow: "hidden",
          background: C.surface,
          boxShadow: `
            0 30px 80px rgba(0,0,0,0.6),
            0 0 0 1px ${C.border},
            0 0 60px ${tintColors[sceneIdx] || "transparent"}
          `,
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* Title bar */}
        <div
          style={{
            height: 40,
            background: C.titleBg,
            borderBottom: `1px solid ${C.border}`,
            display: "flex",
            alignItems: "center",
            padding: "0 16px",
            flexShrink: 0,
          }}
        >
          <div style={{ display: "flex", gap: 8 }}>
            {["#ff5f56", "#ffbd2e", "#27c93f"].map((c) => (
              <div
                key={c}
                style={{
                  width: 12,
                  height: 12,
                  borderRadius: "50%",
                  backgroundColor: c,
                  opacity: 0.9,
                }}
              />
            ))}
          </div>
          <div
            style={{
              flex: 1,
              textAlign: "center",
              fontFamily: UI_FONT,
              fontSize: 13,
              color: C.dim,
              fontWeight: 500,
            }}
          >
            {title}
          </div>
          <div style={{ width: 12 * 3 + 16 }} />
        </div>

        {/* Content */}
        <div style={{ flex: 1, position: "relative", padding: "8px 0" }}>
          {children}
        </div>
      </div>
    </div>
  );
}

function Dots({ frame, total }: { frame: number; total: number }) {
  const sceneIdx = Math.min(Math.floor(frame / SCENE_DURATION), total - 1);
  return (
    <div
      style={{
        position: "absolute",
        bottom: 20,
        left: "50%",
        transform: "translateX(-50%)",
        display: "flex",
        gap: 8,
        alignItems: "center",
      }}
    >
      {Array.from({ length: total }).map((_, i) => {
        const isActive = i === sceneIdx;
        const w = isActive
          ? interpolate(
              Math.min(20, frame % SCENE_DURATION),
              [0, 20],
              [8, 28]
            )
          : 8;
        return (
          <div
            key={i}
            style={{
              width: w,
              height: 5,
              borderRadius: 3,
              backgroundColor: isActive
                ? C.accent
                : i < sceneIdx
                  ? C.accent + "50"
                  : C.border,
              transition: "none",
              boxShadow: isActive ? `0 0 6px ${C.accent}60` : "none",
            }}
          />
        );
      })}
    </div>
  );
}

export const JetDemo: React.FC = () => {
  const frame = useCurrentFrame();
  const sceneIdx = Math.min(
    Math.floor(frame / SCENE_DURATION),
    SCENES.length - 1
  );
  const base = sceneIdx * SCENE_DURATION;
  const scene = SCENES[sceneIdx];

  return (
    <AbsoluteFill
      style={{
        backgroundColor: C.bg,
        backgroundImage: `
          radial-gradient(ellipse 80% 50% at 50% 0%, rgba(88,166,255,0.06) 0%, transparent 70%),
          radial-gradient(ellipse 60% 40% at 50% 100%, rgba(188,140,255,0.04) 0%, transparent 60%)
        `,
      }}
    >
      {/* Subtle grid */}
      <div
        style={{
          position: "absolute",
          inset: 0,
          backgroundImage: `
            linear-gradient(rgba(30,38,48,0.3) 1px, transparent 1px),
            linear-gradient(90deg, rgba(30,38,48,0.3) 1px, transparent 1px)
          `,
          backgroundSize: "40px 40px",
        }}
      />

      <TerminalFrame title={SCENE_NAMES[sceneIdx]} frame={frame} sceneIdx={sceneIdx}>
        <SceneLines
          lines={scene.lines}
          frame={frame}
          base={base + 8}
          sceneIdx={sceneIdx}
        />

        {/* Cursor */}
        {frame % 50 < 35 && (
          <div
            style={{
              position: "absolute",
              left: 28,
              bottom: 48,
              width: 8,
              height: 18,
              backgroundColor: C.cursor,
              borderRadius: 1,
              opacity: 0.75,
              boxShadow: `0 0 6px ${C.cursor}`,
            }}
          />
        )}
      </TerminalFrame>

      <Dots frame={frame} total={SCENES.length} />
    </AbsoluteFill>
  );
};
