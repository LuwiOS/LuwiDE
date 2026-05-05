mod shell;

use efame::NativeOptions;

fn main() -> anyhow::Result<()> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_title("LuwiDE Shell"),
        ..Default::default()
    };

    efame::run_native(
        "LuwiDE",
        options,
        Box::new(|cc| Ok(Box::new(shell::LuwiShell::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}
