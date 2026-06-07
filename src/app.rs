use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    export,
    parser::{self, TableData},
};
use egui::{Color32, Frame, Margin, RichText, ScrollArea, TextStyle};
use egui_extras::{Column, TableBuilder};
use rfd::FileDialog;

const UTF8_BOM: &str = "\u{feff}";

pub struct CrabPasteApp {
    input_text: String,
    table_data: Option<TableData>,
    source_name: String,
    source_stem: String,
    parse_status: String,
    parse_status_color: Color32,
    export_status: String,
    export_status_color: Color32,
    dark_mode: bool,
    k_sep_toggle: bool,
}

impl Default for CrabPasteApp {
    fn default() -> Self {
        Self {
            input_text: String::new(),
            table_data: None,
            source_name: "粘贴内容".to_string(),
            source_stem: "crabpaste".to_string(),
            parse_status: "等待输入 JSON".to_string(),
            parse_status_color: Color32::GRAY,
            export_status: "解析后可复制或保存。".to_string(),
            export_status_color: Color32::GRAY,
            dark_mode: false,
            k_sep_toggle: false,
        }
    }
}

impl CrabPasteApp {
    fn parse_current(&mut self) {
        if self.input_text.trim().is_empty() {
            self.table_data = None;
            self.set_parse_error("输入为空：请粘贴或打开 JSON。");
            self.set_export_info("没有可导出的表格。");
            return;
        }

        let force_k_sep = self.k_sep_toggle.then_some(true);
        match parser::parse(&self.input_text, force_k_sep) {
            Ok(data) => {
                let summary = format!(
                    "解析成功：{}，{} 行，{} 列，来源：{}。",
                    data.source_format.display_name(),
                    data.row_count(),
                    data.column_count(),
                    self.source_name
                );
                self.table_data = Some(data);
                self.set_parse_success(&summary);
                self.set_export_info("可复制 TSV / Excel 富文本，或保存 XLSX / CSV / TSV。");
            }
            Err(e) => {
                self.table_data = None;
                self.set_parse_error(&format_parse_error(&e));
                self.set_export_info("解析失败，无法导出。");
            }
        }
    }

    fn clear(&mut self) {
        self.input_text.clear();
        self.table_data = None;
        self.source_name = "粘贴内容".to_string();
        self.source_stem = "crabpaste".to_string();
        self.set_parse_info("等待输入 JSON");
        self.set_export_info("解析后可复制或保存。");
    }

    fn open_json_file(&mut self) {
        match FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("payload.json")
            .pick_file()
        {
            Some(path) => self.load_json_path(&path),
            None => self.set_parse_info("已取消打开。"),
        }
    }

    fn load_json_path(&mut self, path: &Path) {
        if !is_json_file(path) {
            self.set_parse_error("仅支持打开 .json 文件。");
            return;
        }

        match fs::read_to_string(path) {
            Ok(content) => {
                self.input_text = content;
                self.set_source_from_path(path);
                self.parse_current();
            }
            Err(e) => {
                self.table_data = None;
                self.set_parse_error(&format!("读取文件失败：{e}"));
                self.set_export_info("没有可导出的表格。");
            }
        }
    }

    fn paste_from_clipboard(&mut self) {
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
            Ok(text) if text.trim().is_empty() => {
                self.table_data = None;
                self.source_name = "剪贴板".to_string();
                self.source_stem = "crabpaste".to_string();
                self.set_parse_error("剪贴板为空：没有可解析的 JSON。");
                self.set_export_info("没有可导出的表格。");
            }
            Ok(text) => {
                self.input_text = text;
                self.source_name = "剪贴板".to_string();
                self.source_stem = "crabpaste".to_string();
                self.parse_current();
            }
            Err(e) => {
                self.set_parse_error(&format!("读取剪贴板失败：{e}"));
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped_files.is_empty() {
            return;
        }

        let json_file = dropped_files
            .iter()
            .find(|file| file.path.as_deref().is_some_and(is_json_file));

        match json_file.and_then(|file| file.path.as_deref()) {
            Some(path) => self.load_json_path(path),
            None => self.set_parse_error("仅支持拖入 .json 文件。"),
        }
    }

    fn mark_input_edited(&mut self) {
        self.source_name = "粘贴内容".to_string();
        self.source_stem = "crabpaste".to_string();
        if self.table_data.is_some() {
            self.table_data = None;
            self.set_parse_info("内容已修改，请重新解析。");
            self.set_export_info("重新解析后可复制或保存。");
        }
    }

    fn copy_tsv(&mut self) {
        let Some(data) = &self.table_data else {
            self.set_export_error("没有可复制的数据，请先解析 JSON。");
            return;
        };

        let tsv = export::to_tsv(data);
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(tsv)) {
            Ok(()) => self.set_export_success("已复制 TSV，可直接粘贴到 Excel/WPS。"),
            Err(e) => self.set_export_error(&format!("剪贴板写入失败：{e}")),
        }
    }

    fn copy_excel_html(&mut self) {
        let Some(data) = &self.table_data else {
            self.set_export_error("没有可复制的数据，请先解析 JSON。");
            return;
        };

        let tsv = export::to_tsv(data);
        let html = export::to_html_table(data);
        match arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_html(html, Some(tsv)))
        {
            Ok(()) => self.set_export_success("已复制 Excel 富文本，可粘贴为表格。"),
            Err(e) => self.set_export_error(&format!("剪贴板写入失败：{e}")),
        }
    }

    fn save_xlsx(&mut self) {
        let Some(path) = self.choose_save_path("Excel 工作簿", &["xlsx"], "xlsx") else {
            self.set_export_info("已取消保存。");
            return;
        };

        let Some(data) = &self.table_data else {
            self.set_export_error("没有可保存的数据，请先解析 JSON。");
            return;
        };

        match export::save_xlsx(data, &path) {
            Ok(()) => self.set_export_success(&format!("已保存 XLSX：{}", display_path(&path))),
            Err(e) => self.set_export_error(&format!("保存 XLSX 失败：{e}")),
        }
    }

    fn save_csv(&mut self) {
        let Some(path) = self.choose_save_path("CSV", &["csv"], "csv") else {
            self.set_export_info("已取消保存。");
            return;
        };

        let Some(data) = &self.table_data else {
            self.set_export_error("没有可保存的数据，请先解析 JSON。");
            return;
        };

        match export::to_csv(data)
            .and_then(|csv| fs::write(&path, format!("{UTF8_BOM}{csv}")).map_err(Into::into))
        {
            Ok(()) => self.set_export_success(&format!("已保存 CSV：{}", display_path(&path))),
            Err(e) => self.set_export_error(&format!("保存 CSV 失败：{e}")),
        }
    }

    fn save_tsv(&mut self) {
        let Some(path) = self.choose_save_path("TSV", &["tsv", "txt"], "tsv") else {
            self.set_export_info("已取消保存。");
            return;
        };

        let Some(data) = &self.table_data else {
            self.set_export_error("没有可保存的数据，请先解析 JSON。");
            return;
        };

        let tsv = export::to_tsv(data);
        match fs::write(&path, format!("{UTF8_BOM}{tsv}")) {
            Ok(()) => self.set_export_success(&format!("已保存 TSV：{}", display_path(&path))),
            Err(e) => self.set_export_error(&format!("保存 TSV 失败：{e}")),
        }
    }

    fn choose_save_path(
        &self,
        label: &str,
        extensions: &[&str],
        extension: &str,
    ) -> Option<PathBuf> {
        FileDialog::new()
            .add_filter(label, extensions)
            .set_file_name(self.default_export_name(extension))
            .save_file()
            .map(|path| ensure_extension(path, extension))
    }

    fn default_export_name(&self, extension: &str) -> String {
        format!("{}.{}", self.source_stem, extension)
    }

    fn set_source_from_path(&mut self, path: &Path) {
        self.source_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("JSON 文件")
            .to_string();
        self.source_stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| !stem.is_empty())
            .unwrap_or("crabpaste")
            .to_string();
    }

    fn set_parse_error(&mut self, msg: &str) {
        self.parse_status = msg.to_string();
        self.parse_status_color = Color32::RED;
    }

    fn set_parse_info(&mut self, msg: &str) {
        self.parse_status = msg.to_string();
        self.parse_status_color = Color32::GRAY;
    }

    fn set_parse_success(&mut self, msg: &str) {
        self.parse_status = msg.to_string();
        self.parse_status_color = Color32::DARK_GREEN;
    }

    fn set_export_error(&mut self, msg: &str) {
        self.export_status = msg.to_string();
        self.export_status_color = Color32::RED;
    }

    fn set_export_info(&mut self, msg: &str) {
        self.export_status = msg.to_string();
        self.export_status_color = Color32::GRAY;
    }

    fn set_export_success(&mut self, msg: &str) {
        self.export_status = msg.to_string();
        self.export_status_color = Color32::DARK_GREEN;
    }
}

impl eframe::App for CrabPasteApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.window_fill().to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        ctx.set_visuals(if self.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });

        self.handle_dropped_files(&ctx);
        set_text_styles(&ctx);

        let total_height = ui.available_height();
        let input_height = (total_height * 0.42).clamp(190.0, 520.0);

        Frame::group(ui.style())
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.set_max_height(input_height);

                ui.horizontal(|ui| {
                    ui.heading("CrabPaste");
                    ui.separator();
                    ui.label("BI/DSP JSON 转 Excel");
                });
                ui.separator();

                ui.horizontal_wrapped(|ui| {
                    if ui.button("打开 JSON").clicked() {
                        self.open_json_file();
                    }
                    if ui.button("粘贴剪贴板").clicked() {
                        self.paste_from_clipboard();
                    }
                    if ui.button("解析").clicked() {
                        self.parse_current();
                    }
                    if ui.button("清空").clicked() {
                        self.clear();
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.k_sep_toggle, "千位分隔符").changed()
                        && !self.input_text.trim().is_empty()
                    {
                        self.parse_current();
                    }
                    ui.checkbox(&mut self.dark_mode, "黑暗模式");
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("来源：{}", self.source_name));
                    if let Some(data) = &self.table_data {
                        ui.separator();
                        ui.label(format!("格式：{}", data.source_format.display_name()));
                        ui.label(format!("{} 行", data.row_count()));
                        ui.label(format!("{} 列", data.column_count()));
                    }
                });

                ui.colored_label(self.parse_status_color, &self.parse_status);
                ui.separator();

                let used_height = 112.0;
                let scroll_h = (input_height - used_height).max(70.0);
                ScrollArea::vertical()
                    .id_salt("input_scroll")
                    .max_height(scroll_h)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        let response = ui.add(
                            egui::TextEdit::multiline(&mut self.input_text)
                                .hint_text("粘贴 BI/DSP 响应 JSON...")
                                .font(TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                        if response.changed() {
                            self.mark_input_edited();
                        }
                    });
            });

        ui.separator();

        if self.table_data.is_some() {
            ui.horizontal_wrapped(|ui| {
                if ui.button("复制 TSV").clicked() {
                    self.copy_tsv();
                }
                if ui.button("复制 Excel 富文本").clicked() {
                    self.copy_excel_html();
                }
                ui.separator();
                if ui.button("保存 XLSX").clicked() {
                    self.save_xlsx();
                }
                if ui.button("保存 CSV").clicked() {
                    self.save_csv();
                }
                if ui.button("保存 TSV").clicked() {
                    self.save_tsv();
                }
            });
            ui.colored_label(self.export_status_color, &self.export_status);
            ui.separator();
        }

        if let Some(data) = &self.table_data {
            render_table(ui, data);
        } else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("粘贴、打开或拖入 JSON 后点击解析")
                        .color(Color32::GRAY)
                        .size(16.0),
                );
            });
        }
    }
}

fn render_table(ui: &mut egui::Ui, data: &TableData) {
    let header_height = 26.0;
    let row_height = 22.0;
    let table_height = ui.available_height().max(220.0);
    let min_table_width = (data.columns.len() as f32 * 120.0).max(ui.available_width());

    ScrollArea::horizontal()
        .id_salt("table_horizontal_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(min_table_width);
            let column = Column::auto_with_initial_suggestion(140.0)
                .at_least(80.0)
                .at_most(340.0)
                .clip(true);

            TableBuilder::new(ui)
                .id_salt("parsed_table")
                .striped(true)
                .resizable(true)
                .auto_shrink([false, false])
                .min_scrolled_height(160.0)
                .max_scroll_height(table_height)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .columns(column, data.columns.len())
                .header(header_height, |mut header| {
                    for col in &data.columns {
                        header.col(|ui| {
                            ui.strong(&col.name);
                        });
                    }
                })
                .body(|body| {
                    body.rows(row_height, data.rows.len(), |mut row| {
                        let row_idx = row.index();
                        if let Some(row_data) = data.rows.get(row_idx) {
                            for col_idx in 0..data.columns.len() {
                                row.col(|ui| {
                                    ui.label(row_data.get(col_idx).map_or("", String::as_str));
                                });
                            }
                        }
                    });
                });
        });
}

fn set_text_styles(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    style.text_styles.insert(
        TextStyle::Monospace,
        egui::FontId::new(12.0, egui::FontFamily::Monospace),
    );
    ctx.set_global_style(style);
}

fn is_json_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
}

fn ensure_extension(mut path: PathBuf, extension: &str) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension(extension);
    }
    path
}

fn display_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| path.to_string_lossy().into_owned(), ToString::to_string)
}

fn format_parse_error(error: &str) -> String {
    if error.starts_with("JSON parse error") {
        format!(
            "JSON 格式错误：{}",
            error.trim_start_matches("JSON parse error: ")
        )
    } else if error.starts_with("Unknown JSON format") {
        "未知 JSON 格式：未找到 vizData、resultList 或 dataList。".to_string()
    } else if error.starts_with("Missing") {
        format!("JSON 结构不完整：{error}")
    } else {
        format!("解析失败：{error}")
    }
}
