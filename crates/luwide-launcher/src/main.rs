use efame::NativeOptions;
use egui::{Color32, RichText};
use luwide_common::{theme::XpTheme, ConfigStore};

fn main() -> anyhow::Result<()> {
    efame::run_native("LuwiDE Launcher", NativeOptions::default(), Box::new(|_| Ok(Box::new(LauncherApp::new()))))
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

struct LauncherApp { config: Option<ConfigStore> }
impl LauncherApp { fn new() -> Self { Self { config: ConfigStore::load_or_create().ok() } } }

impl efame::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut efame::Frame) {
        if let Some(cfg) = &mut self.config { let _ = cfg.reload_if_changed(); }

        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.max_rect();
            ui.painter().rect_filled(rect, 8.0, XpTheme::WINDOW_BG);
            ui.painter().rect_stroke(rect, 8.0, (2.0, XpTheme::WINDOW_EDGE), egui::StrokeKind::Inside);
            ui.heading(RichText::new("Menu Iniciar").color(XpTheme::WINDOW_EDGE));
            ui.separator();
            if let Some(cfg) = &self.config { ui.label(format!("Budget launcher: {}MB", cfg.current().memory_budget.launcher_mb)); }
            let apps = self.config.as_ref().map(|c| c.current().apps.clone()).unwrap_or_default();
            for app in apps {
                ui.add(egui::Button::new(format!("📦 {}", app.title)).fill(Color32::from_rgb(221, 233, 255)));
            }
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
}
