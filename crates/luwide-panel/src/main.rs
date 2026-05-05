use efame::NativeOptions;
use egui::{Color32, RichText};
use luwide_common::{theme::XpTheme, ConfigStore, SessionSnapshot};

fn main() -> anyhow::Result<()> {
    efame::run_native(
        "LuwiDE Panel",
        NativeOptions::default(),
        Box::new(|_| Ok(Box::new(PanelApp::new()))),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}

struct PanelApp {
    running: Vec<String>,
    config: Option<ConfigStore>,
}

impl PanelApp { fn new() -> Self { Self { running: vec![], config: ConfigStore::load_or_create().ok() } } }

impl efame::App for PanelApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut efame::Frame) {
        if self.running.is_empty() { self.running = SessionSnapshot::new(1, vec!["Explorer".into(), "Terminal".into()]).running_apps; }
        if let Some(cfg) = &mut self.config { let _ = cfg.reload_if_changed(); }

        let (height, start_label, show_clock) = self.config.as_ref().map(|cfg| {
            let p = &cfg.current().panel; (p.height, p.start_label.clone(), p.show_clock)
        }).unwrap_or((40.0, "Iniciar".into(), true));

        egui::TopBottomPanel::bottom("taskbar").exact_height(height).show(ctx, |ui| {
            let rect = ui.max_rect();
            ui.painter().rect_filled(rect, 0.0, XpTheme::TASKBAR);
            ui.painter().line_segment([rect.left_top(), rect.right_top()], (1.0, XpTheme::TASKBAR_HIGHLIGHT));
            ui.horizontal(|ui| {
                ui.add(egui::Button::new(RichText::new(start_label).strong().color(Color32::WHITE)).fill(XpTheme::START_BUTTON).stroke((1.0, XpTheme::START_BUTTON_EDGE)).corner_radius(egui::CornerRadius::same(12)));
                for app in &self.running {
                    ui.add(egui::Button::new(RichText::new(format!("◼ {}", app)).color(Color32::WHITE)).fill(XpTheme::TASKBAR_HIGHLIGHT));
                }
                if let Some(cfg) = &self.config {
                    ui.label(RichText::new(format!("Budget panel: {}MB", cfg.current().memory_budget.panel_mb)).color(Color32::WHITE));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("🔋 📶 🔊").color(Color32::WHITE));
                    if show_clock { ui.label(RichText::new(SessionSnapshot::new(1, vec![]).clock).color(Color32::WHITE)); }
                });
            });
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
}
