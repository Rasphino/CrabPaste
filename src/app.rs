use crate::parser::{self, TableData};
use egui::{Color32, Frame, Margin, RichText, ScrollArea, TextStyle};
use egui_extras::{Column, TableBuilder};

pub struct CrabPasteApp {
    input_text: String,
    table_data: Option<TableData>,
    status: String,
    status_color: Color32,
    row_count: usize,
    col_count: usize,
}

impl Default for CrabPasteApp {
    fn default() -> Self {
        Self {
            input_text: String::new(),
            table_data: None,
            status: String::new(),
            status_color: Color32::GRAY,
            row_count: 0,
            col_count: 0,
        }
    }
}

impl CrabPasteApp {
    fn parse(&mut self) {
        if self.input_text.trim().is_empty() {
            self.set_error("Input is empty");
            return;
        }

        match parser::parse(&self.input_text) {
            Ok(data) => {
                self.row_count = data.rows.len();
                self.col_count = data.columns.len();
                self.table_data = Some(data);
                self.set_info(&format!(
                    "Parsed {} rows, {} columns",
                    self.row_count, self.col_count
                ));
            }
            Err(e) => {
                self.table_data = None;
                self.set_error(&e);
            }
        }
    }

    fn clear(&mut self) {
        self.input_text.clear();
        self.table_data = None;
        self.status.clear();
    }

    fn copy_to_clipboard(&mut self) {
        let data = match &self.table_data {
            Some(d) => d,
            None => {
                self.set_error("No data to copy. Parse JSON first.");
                return;
            }
        };

        let tsv = to_tsv(data);

        match arboard::Clipboard::new() {
            Ok(mut clipboard) => {
                if let Err(e) = clipboard.set_text(&tsv) {
                    self.set_error(&format!("Clipboard error: {}", e));
                } else {
                    self.set_success("Copied! Paste into Excel.");
                }
            }
            Err(e) => {
                self.set_error(&format!("Clipboard error: {}", e));
            }
        }
    }

    fn set_error(&mut self, msg: &str) {
        self.status = msg.to_string();
        self.status_color = Color32::RED;
    }

    fn set_info(&mut self, msg: &str) {
        self.status = msg.to_string();
        self.status_color = Color32::GRAY;
    }

    fn set_success(&mut self, msg: &str) {
        self.status = msg.to_string();
        self.status_color = Color32::DARK_GREEN;
    }
}

fn to_tsv(data: &TableData) -> String {
    let mut lines = Vec::new();

    // Header row
    let headers: Vec<&str> = data.columns.iter().map(|c| c.name.as_str()).collect();
    lines.push(headers.join("\t"));

    // Data rows
    for row in &data.rows {
        lines.push(row.join("\t"));
    }

    let mut tsv = lines.join("\n");

    // Ensure trailing newline for Excel compatibility
    tsv.push('\n');
    tsv
}

impl eframe::App for CrabPasteApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Set up text styles
        let mut style = (*ctx.global_style()).clone();
        style.text_styles.insert(
            TextStyle::Monospace,
            egui::FontId::new(12.0, egui::FontFamily::Monospace),
        );
        ctx.set_global_style(style);

        let total_height = ui.available_height();
        // Input area: ~45% of window height, clamped to sane bounds
        let input_height = (total_height * 0.45).clamp(180.0, 600.0);

        // === Input Section (fixed height) ===
        Frame::group(ui.style())
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.set_max_height(input_height);

                ui.horizontal(|ui| {
                    ui.heading("CrabPaste");
                    ui.separator();
                    ui.label("BI Table Paster — paste JSON, get a copyable table");
                });
                ui.separator();

                // Compute height budget for the scroll area
                let used_height = 56.0; // title + button row + separators
                let scroll_h = (input_height - used_height).max(50.0);

                ScrollArea::vertical()
                    .id_salt("input_scroll")
                    .max_height(scroll_h)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.add(
                            egui::TextEdit::multiline(&mut self.input_text)
                                .hint_text("Paste BI response JSON here...")
                                .font(TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                    });

                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Parse").clicked() {
                        self.parse();
                    }
                    if ui.button("Clear").clicked() {
                        self.clear();
                    }
                    ui.separator();
                    ui.colored_label(self.status_color, &self.status);
                });
            });

        ui.separator();

        // === Table Section (fills remaining space) ===
        // Copy button — separate if-check to avoid borrow conflict with table rendering
        if self.table_data.is_some() {
            ui.horizontal(|ui| {
                let copy_btn = egui::Button::new("Copy to Clipboard (TSV)")
                    .fill(Color32::from_rgb(64, 128, 64));
                if ui.add(copy_btn).clicked() {
                    self.copy_to_clipboard();
                }
            });
            ui.separator();
        }

        if let Some(data) = &self.table_data {
            let header_height = 24.0;
            let row_height = 20.0;
            let num_rows = data.rows.len();

            ScrollArea::vertical()
                .id_salt("table_scroll")
                .show(ui, |ui| {
                    TableBuilder::new(ui)
                        .striped(true)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .columns(Column::auto(), data.columns.len())
                        .header(header_height, |mut header| {
                            for col in &data.columns {
                                header.col(|ui| {
                                    ui.strong(&col.name);
                                });
                            }
                        })
                        .body(|body| {
                            body.rows(row_height, num_rows, |mut row| {
                                let row_idx = row.index();
                                if let Some(row_data) = data.rows.get(row_idx) {
                                    for cell in row_data {
                                        row.col(|ui| {
                                            ui.label(cell);
                                        });
                                    }
                                }
                            });
                        });
                });
        } else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Paste JSON above and click Parse")
                        .color(Color32::GRAY)
                        .size(16.0),
                );
            });
        }
    }
}
