//! `luwide-panel` — barra de tarefas estilo XP do LuwiDE (processo separado).
//!
//! Mostra o botão Iniciar (abre o `luwide-launcher`), os aplicativos
//! externos em execução (via IPC com o sessiond) e a bandeja com relógio.
//! Na sessão integrada, quem cumpre esse papel é a própria shell; o panel
//! autônomo serve para sessões com outros compositores/shells.

use egui::{Color32, FontId, Id, Pos2, Rect, Rounding, Sense, Stroke, Vec2};
use luwide_common::{
    ipc::{IpcRequest, IpcResponse},
    theme::XpTheme,
    ConfigStore, SessionSnapshot,
};
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 40.0])
            .with_decorations(false)
            .with_title("LuwiDE Panel"),
        ..Default::default()
    };
    eframe::run_native("LuwiDE Panel", options, Box::new(|cc| {
        XpTheme::apply(&cc.egui_ctx);
        Ok(Box::new(PanelApp::new()))
    }))
    .map_err(|e| anyhow::anyhow!(e.to_string()))
}

struct PanelApp {
    config: Option<ConfigStore>,
    session: Option<SessionSnapshot>,
    last_poll: Instant,
    status: String,
}

impl PanelApp {
    fn new() -> Self {
        Self {
            config: ConfigStore::load_or_create().ok(),
            session: None,
            last_poll: Instant::now() - Duration::from_secs(10),
            status: String::new(),
        }
    }

    fn poll(&mut self) {
        if self.last_poll.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.last_poll = Instant::now();
        if let Ok(IpcResponse::Snapshot { snapshot }) =
            luwide_common::ipc::request(&IpcRequest::GetSnapshot)
        {
            self.session = Some(snapshot);
        } else {
            self.session = None;
        }
    }
}

impl eframe::App for PanelApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(cfg) = &mut self.config {
            let _ = cfg.reload_if_changed();
        }
        self.poll();
        let (height, start_label, show_clock) = self
            .config
            .as_ref()
            .map(|c| {
                let p = &c.current().panel;
                (p.height, p.start_label.clone(), p.show_clock)
            })
            .unwrap_or((40.0, "Iniciar".into(), true));

        egui::CentralPanel::default()
            .frame(egui::Frame::default().inner_margin(egui::Margin::ZERO))
            .show(ctx, |ui| {
                let full = ui.max_rect();
                let painter = ui.painter();
                XpTheme::gradient_vertical(painter, full, XpTheme::TASKBAR_HIGHLIGHT, XpTheme::TASKBAR, 10);
                painter.line_segment(
                    [full.left_top(), full.right_top()],
                    Stroke::new(2.0, Color32::from_rgb(140, 190, 255)),
                );

                // Botão Iniciar → abre o launcher (binário irmão deste).
                let start = Rect::from_min_size(
                    Pos2::new(full.min.x + 2.0, full.min.y + 3.0),
                    Vec2::new(118.0, height - 6.0),
                );
                let resp = ui.interact(start, Id::new("panel-start"), Sense::click());
                XpTheme::gradient_vertical(painter, start, XpTheme::START_BUTTON_LIGHT, XpTheme::START_BUTTON, 8);
                painter.rect_stroke(start, Rounding::same(6.0), Stroke::new(1.0, XpTheme::START_BUTTON_EDGE));
                painter.text(start.center(), egui::Align2::CENTER_CENTER, format!("🏁 {start_label}"), FontId::proportional(15.0), Color32::WHITE);
                if resp.clicked() {
                    self.status = match launch_launcher() {
                        Ok(()) => "Menu Iniciar aberto".into(),
                        Err(e) => format!("launcher indisponível: {e}"),
                    };
                }

                // Apps em execução (sessiond) ou aviso de modo degradado
                let mut x = start.max.x + 10.0;
                if let Some(session) = &self.session {
                    for app in &session.running_apps {
                        if x + 130.0 > full.max.x - 180.0 {
                            break;
                        }
                        let rect = Rect::from_min_size(Pos2::new(x, full.min.y + 5.0), Vec2::new(130.0, height - 10.0));
                        painter.rect_filled(rect, Rounding::same(3.0), Color32::from_rgba_unmultiplied(255, 255, 255, 38));
                        painter.text(
                            Pos2::new(rect.min.x + 8.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            format!("▶ {}", app.title),
                            FontId::proportional(11.5),
                            Color32::WHITE,
                        );
                        x += 136.0;
                    }
                    if session.running_apps.is_empty() {
                        painter.text(Pos2::new(x, full.center().y), egui::Align2::LEFT_CENTER, "sessiond conectado 🟢", FontId::proportional(11.5), Color32::WHITE);
                    }
                } else {
                    painter.text(
                        Pos2::new(x, full.center().y),
                        egui::Align2::LEFT_CENTER,
                        if self.status.is_empty() { "sessiond offline ⚪ (inicie luwide-sessiond)" } else { &self.status },
                        FontId::proportional(11.5),
                        Color32::WHITE,
                    );
                }

                // Bandeja
                let tray = Rect::from_min_max(
                    Pos2::new(full.max.x - 170.0, full.min.y + 3.0),
                    Pos2::new(full.max.x - 3.0, full.max.y - 3.0),
                );
                painter.rect_filled(tray, Rounding::same(4.0), XpTheme::TRAY_BG);
                let text = if show_clock {
                    format!("🔊 📶 🔋  {}", luwide_common::now_clock())
                } else {
                    "🔊 📶 🔋".to_string()
                };
                painter.text(tray.center(), egui::Align2::CENTER_CENTER, text, FontId::proportional(12.5), Color32::WHITE);
            });
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

fn launch_launcher() -> std::io::Result<()> {
    // Procura `luwide-launcher` ao lado do executável atual; depois no PATH.
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("luwide-launcher")))
        .filter(|p| p.exists());
    let program = sibling
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "luwide-launcher".into());
    std::process::Command::new(program)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}
