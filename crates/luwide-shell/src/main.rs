mod apps;
mod calc;
mod shell;
mod wm;

use eframe::NativeOptions;

fn main() -> anyhow::Result<()> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("LuwiDE — Ambiente de Trabalho"),
        ..Default::default()
    };
    eframe::run_native(
        "LuwiDE",
        options,
        Box::new(|cc| Ok(Box::new(shell::LuwiShell::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}
