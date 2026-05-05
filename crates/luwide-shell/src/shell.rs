use egui::{Color32, RichText};
use luwide_common::{default_apps, theme::XpTheme, ConfigStore};

pub struct LuwiShell { config: Option<ConfigStore> }
impl LuwiShell { pub fn new(_cc: &efame::CreationContext<'_>) -> Self { Self { config: ConfigStore::load_or_create().ok() } } }

impl efame::App for LuwiShell {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut efame::Frame) {
        if let Some(cfg) = &mut self.config { let _ = cfg.reload_if_changed(); }
        egui::CentralPanel::default().show(ctx, |ui| {
            let rect = ui.max_rect();
            let horizon = rect.top() + rect.height() * 0.72;
            ui.painter().rect_filled(egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, horizon)), 0.0, XpTheme::DESKTOP_SKY);
            ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(rect.min.x, horizon), rect.max), 0.0, XpTheme::DESKTOP_GRASS);

            ui.vertical(|ui| {
                ui.add_space(18.0);
                for app in default_apps() {
                    ui.label(RichText::new(format!("🗂 {}", app.title)).color(Color32::WHITE).size(18.0));
                    ui.add_space(6.0);
                }
            });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                if let Some(cfg) = &self.config { ui.label(RichText::new(format!("Budget shell: {}MB", cfg.current().memory_budget.shell_mb)).color(Color32::WHITE)); }
                ui.label(RichText::new("LuwiDE Nostalgia Theme").color(Color32::WHITE));
            });
        });
    }
}
