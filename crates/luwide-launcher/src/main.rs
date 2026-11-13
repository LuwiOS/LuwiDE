//! `luwide-launcher` — Menu Iniciar do LuwiDE (processo separado).
//!
//! Lista os programas reais da máquina (config + arquivos `.desktop`),
//! com busca, e lança o escolhido através do sessiond (que rastreia o
//! processo). Sem sessiond, lança direto. Fecha ao concluir.

use egui::{Color32, RichText, Rounding, Stroke};
use luwide_common::{
    discovery::merged_apps,
    ipc::{IpcRequest, IpcResponse},
    theme::XpTheme,
    ConfigStore, DesktopEntry,
};

fn main() -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 540.0])
            .with_title("Menu Iniciar — LuwiDE"),
        ..Default::default()
    };
    eframe::run_native("LuwiDE Launcher", options, Box::new(|cc| {
        XpTheme::apply(&cc.egui_ctx);
        Ok(Box::new(LauncherApp::new()))
    }))
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}

struct LauncherApp {
    config: Option<ConfigStore>,
    apps: Vec<DesktopEntry>,
    search: String,
    status: String,
}

impl LauncherApp {
    fn new() -> Self {
        let config = ConfigStore::load_or_create().ok();
        let config_apps = config
            .as_ref()
            .map(|c| c.current().apps.clone())
            .unwrap_or_else(luwide_common::default_apps);
        Self { apps: merged_apps(&config_apps), config, search: String::new(), status: String::new() }
    }

    fn launch(&mut self, app: &DesktopEntry, ctx: &egui::Context) {
        if app.builtin {
            // Apps internos vivem na shell; pedimos ao sessiond apenas o
            // registro se ele estiver ativo, e orientamos o usuário.
            self.status = format!("'{}' é um app interno: abra-o pela shell do LuwiDE.", app.title);
            return;
        }
        let result = luwide_common::ipc::request(&IpcRequest::LaunchApp {
            exec: app.exec.clone(),
            title: app.title.clone(),
        });
        match result {
            Ok(IpcResponse::Launched { app: running }) => {
                self.status = format!("{} iniciado (pid {})", app.title, running.pid);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Ok(IpcResponse::Error { message }) => {
                self.status = format!("Erro: {message}");
            }
            _ => match spawn_external(&app.exec) {
                Ok(_) => {
                    self.status = format!("{} iniciado", app.title);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Err(e) => self.status = format!("Não foi possível iniciar {}: {e}", app.title),
            },
        }
    }
}

fn spawn_external(exec: &str) -> std::io::Result<()> {
    let mut parts = exec.split_whitespace();
    let program = parts.next().unwrap_or("");
    if program.is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "comando vazio"));
    }
    std::process::Command::new(program)
        .args(parts)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(cfg) = &mut self.config {
            let _ = cfg.reload_if_changed();
        }
        let user = self
            .config
            .as_ref()
            .map(|c| c.current().theme.user_name.clone())
            .unwrap_or_else(|| "Usuário".into());

        egui::CentralPanel::default()
            .frame(egui::Frame::default().inner_margin(egui::Margin::ZERO))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, Rounding::ZERO, XpTheme::WINDOW_BG);
                ui.painter().rect_stroke(rect, Rounding::ZERO, Stroke::new(2.0, XpTheme::WINDOW_EDGE));

                // Cabeçalho azul
                let header = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.min.y + 58.0));
                XpTheme::gradient_vertical(ui.painter(), header, XpTheme::TASKBAR_HIGHLIGHT, XpTheme::TASKBAR, 10);
                ui.painter().text(
                    egui::pos2(header.min.x + 16.0, header.center().y),
                    egui::Align2::LEFT_CENTER,
                    format!("👤 {user}"),
                    egui::FontId::proportional(17.0),
                    Color32::WHITE,
                );

                let body = egui::Rect::from_min_max(
                    egui::pos2(rect.min.x + 10.0, header.max.y + 10.0),
                    egui::pos2(rect.max.x - 10.0, rect.max.y - 54.0),
                );
                let mut clicked: Option<DesktopEntry> = None;
                ui.allocate_ui_at_rect(body, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("🔎 Buscar programa…")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(6.0);
                    let query = self.search.to_lowercase();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.set_width(body.width() - 8.0);
                        let mut shown = 0;
                        for app in &self.apps {
                            if !query.is_empty() && !app.title.to_lowercase().contains(&query) {
                                continue;
                            }
                            shown += 1;
                            let resp = ui.add(
                                egui::Button::new(
                                    RichText::new(format!("{}  {}", app.icon, app.title)).size(14.0),
                                )
                                .fill(Color32::from_rgb(221, 233, 255))
                                .stroke(Stroke::new(0.5, XpTheme::WINDOW_EDGE_INACTIVE)),
                            );
                            if resp.clicked() {
                                clicked = Some(app.clone());
                            }
                            ui.add_space(3.0);
                        }
                        if shown == 0 {
                            ui.label(RichText::new("Nenhum programa encontrado.").color(XpTheme::TEXT_MUTED));
                        }
                    });
                });
                if let Some(app) = clicked {
                    self.launch(&app, ctx);
                }

                // Rodapé
                let footer = egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - 44.0), rect.max);
                XpTheme::gradient_vertical(ui.painter(), footer, XpTheme::TASKBAR, XpTheme::TASKBAR_DARK, 8);
                ui.allocate_ui_at_rect(
                    egui::Rect::from_min_max(
                        egui::pos2(footer.min.x + 10.0, footer.min.y),
                        egui::pos2(footer.max.x - 10.0, footer.max.y),
                    ),
                    |ui| {
                        ui.horizontal_centered(|ui| {
                            if !self.status.is_empty() {
                                ui.label(RichText::new(&self.status).color(Color32::WHITE).size(11.5));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .add(egui::Button::new(RichText::new("⏻ Desligar").color(Color32::WHITE)).fill(XpTheme::CLOSE_BUTTON))
                                    .clicked()
                                {
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                            });
                        });
                    },
                );
            });
    }
}
