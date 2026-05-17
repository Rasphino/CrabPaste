use egui::Visuals;

mod app;
mod parser;

fn load_cjk_font(ctx: &egui::Context) {
    let font_path = if cfg!(target_os = "macos") {
        // Try PingFang first (newer macOS), fall back to STHeiti
        let pingfang = "/System/Library/Fonts/PingFang.ttc";
        if std::path::Path::new(pingfang).exists() {
            pingfang.to_owned()
        } else {
            "/System/Library/Fonts/STHeiti Medium.ttc".to_owned()
        }
    } else if cfg!(target_os = "windows") {
        "C:\\Windows\\Fonts\\msyh.ttc".to_owned()
    } else {
        return;
    };

    if let Ok(bytes) = std::fs::read(&font_path) {
        ctx.add_font(egui::epaint::text::FontInsert::new(
            "cjk",
            egui::FontData::from_owned(bytes),
            vec![
                egui::epaint::text::InsertFontFamily {
                    family: egui::FontFamily::Proportional,
                    priority: egui::epaint::text::FontPriority::Lowest,
                },
                egui::epaint::text::InsertFontFamily {
                    family: egui::FontFamily::Monospace,
                    priority: egui::epaint::text::FontPriority::Lowest,
                },
            ],
        ));
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "CrabPaste - BI Table Paster",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(Visuals::light());
            load_cjk_font(&cc.egui_ctx);
            Ok(Box::new(app::CrabPasteApp::default()))
        }),
    )
}
