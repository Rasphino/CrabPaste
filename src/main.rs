mod app;
mod parser;

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
        Box::new(|_cc| Ok(Box::new(app::CrabPasteApp::default()))),
    )
}
