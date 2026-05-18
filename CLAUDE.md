# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build / Test

```bash
cargo build              # debug build
cargo run                # run locally
cargo test               # run all tests (includes payload.json integration test)
cargo check              # fast compile check without producing binary
```

## Rules
- 非微小改动先做好规划，说明实现方案。
- 需求有歧义、风险高或影响大时，先澄清并获批，再开始写代码。
- 坚持 Spec Coding，避免 Vibe Coding；Plan 只写方案、范围、风险和验收标准，不写实现代码。
- 优先小步迭代；实现与审查分离。
- 完成后可执行 /simplify；必要时使用 /loop。
- 任何修改之后都运行`cargo check`进行测试 

## Architecture

```
src/
├── main.rs    # NativeOptions, light theme, CJK font loading, entry point
├── app.rs     # eframe::App impl — UI layout, drag-drop, clipboard copy
└── parser.rs  # BI JSON → TableData parsing, number formatting
```

**Data flow:** JSON text (pasted or drag-dropped) → `parser::parse()` → `TableData` → egui `TableBuilder` rendering → TSV clipboard export.

**parser.rs** — Parses BI system JSON responses:
- Column metadata from `data.vizData.fieldMap` (name via `alias`, type, format config)
- Column ordering from `data.vizData.locationMap` (dimensions first, then measures)
- Row data from `data.vizData.datasets[]`
- Number formatting honors BI format config: `kSep` (thousands separator), `precision` + `precisionType` (`decimalDigits` or `significantDecimal`)
- Dataset values are stored as JSON strings even for numeric fields — the parser explicitly re-parses them when the field type is `float`/`int`

**app.rs** — Single `CrabPasteApp` struct holding `input_text`, parsed `table_data`, and status:
- Input section (top ~45%): multiline `TextEdit` in `ScrollArea`, Parse/Clear buttons, status label
- Table section (bottom): `TableBuilder` with striped rows, then Copy-to-clipboard button
- Drag-drop: JSON files dropped onto the window are loaded into `input_text`
- Copy outputs TSV format for direct Excel paste compatibility

## Key details

- **egui 0.34 API**: `App::ui(&mut self, ui: &mut Ui, frame)` — not the older `App::update`. Panels use `show_inside`.
- **TableBuilder**: must pre-allocate all columns with `.columns(Column::auto(), count)` matching the actual number of columns rendered, otherwise panic.
- **Light theme**: `cc.egui_ctx.set_visuals(Visuals::light())` controls widget colors, but the window background is controlled by `App::clear_color()` which defaults to near-black. Override it to return `visuals.window_fill().to_normalized_gamma_f32()`.
- **CJK fonts**: loaded at startup via system fonts — PingFang/STHeiti on macOS, Microsoft YaHei on Windows. Registered as `Lowest`-priority fallback for both `Proportional` and `Monospace` families.
- **Windows build**: uses `#![windows_subsystem = "windows"]` to suppress the console window. CI builds via GitHub Actions (`.github/workflows/build.yml`).
