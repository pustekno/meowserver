# 0002 — Slint with the software renderer for the GUI

Status: accepted (2026-10-25)

## Context

PRD §9.2 left the choice between Slint and iced to a Phase 0 measurement.
Criteria: idle RAM, binary size, start time, accessibility, contributor
friendliness.

## Measurement

Same test app in both: a window with a heading and a scrollable list of 40
rows (two labels and a button each). Release build (`opt-level = "s"`, LTO),
Windows 11 Home, measured 4 s after launch, idle CPU over the next 5 s.

| Build | Working set | Private | Idle CPU | CPU to first idle | Binary |
|---|---|---|---|---|---|
| Slint 1.18, default (OpenGL/femtovg) | 114.5 MB | 55.5 MB | 0 ms | — | 9.5 MB |
| iced 0.14, default (wgpu) | 138.4 MB | 86.8 MB | 16 ms | — | 7.0 MB |
| **Slint 1.18, software renderer** | **23.1 MB** | **6.7 MB** | **0 ms** | **47 ms** | 8.9 MB |
| iced 0.14, tiny-skia only | 20.4 MB | 7.5 MB | 0 ms | 94 ms | 2.9 MB |

GPU renderers load graphics drivers into the process, which costs ~100 MB.
Both software builds meet the target of ≤ 40 MB for `meowd` + GUI
(14 + ~23 MB).

## Decision

Slint, features `std`, `backend-winit`, `renderer-software`,
`accessibility`, `compat-1-2`. Licensed to us under GPLv3, which matches the
project license.

Reasons over iced:
- **Accessibility.** Slint exposes widgets to screen readers through
  AccessKit; iced 0.14 has no accessibility support. PRD §8.6 requires it.
- **Start time.** Half the CPU time to the first idle frame.
- **Contributors.** UI lives in declarative `.slint` files separate from
  Rust logic; std widgets follow the OS style.

## Consequences

- The GUI binary is about 6 MB larger than iced would be. Acceptable: it is
  a separate process and not loaded while only the tray runs.
- If a GPU renderer is ever wanted (e.g. for animations), it must be
  re-measured against the RAM target first.
