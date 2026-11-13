//! A shell gráfica do LuwiDE: área de trabalho estilo XP (wallpaper
//! "Bliss", ícones, janelas arrastáveis/redimensionáveis), barra de
//! tarefas com botão Iniciar, Menu Iniciar, bandeja com relógio e os
//! aplicativos internos (Explorer, Terminal, Bloco de Notas,
//! Calculadora, Configurações, Monitor do Sistema e Sobre).

use crate::{
    apps::{ExplorerState, NotepadState, TermAction, TerminalState},
    calc::Calculator,
    wm::{RectF, WindowManager},
};
use egui::{Color32, FontId, Id, Order, Pos2, Rect, RichText, Rounding, Sense, Stroke, Vec2};
use luwide_common::{
    discovery::merged_apps,
    ipc::{IpcRequest, IpcResponse},
    theme::{color_from_hex, XpTheme},
    ConfigStore, DesktopEntry, SessionSnapshot,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

enum AppContent {
    Explorer(ExplorerState),
    Terminal(TerminalState),
    Notepad(NotepadState),
    Calculator(Calculator),
    Settings,
    About,
    Monitor,
}

enum Action {
    Close(u64),
    Minimize(u64),
    ToggleMax(u64),
    Focus(u64),
    TaskbarClick(u64),
    SetRect(u64, RectF),
    OpenApp(DesktopEntry),
    OpenExplorerAt(PathBuf),
    LaunchExternal { exec: String, title: String },
    CloseShell,
    Status(String),
}

pub struct LuwiShell {
    config: Option<ConfigStore>,
    wm: WindowManager,
    contents: HashMap<u64, AppContent>,
    all_apps: Vec<DesktopEntry>,
    start_open: bool,
    start_search: String,
    selected_icon: Option<String>,
    status: Option<(String, Instant)>,
    session: Option<SessionSnapshot>,
    last_poll: Instant,
    started: Instant,
}

impl LuwiShell {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        XpTheme::apply(&cc.egui_ctx);
        let config = ConfigStore::load_or_create().ok();
        let config_apps = config
            .as_ref()
            .map(|c| c.current().apps.clone())
            .unwrap_or_else(luwide_common::default_apps);
        let all_apps = merged_apps(&config_apps);
        let mut shell = Self {
            config,
            wm: WindowManager::new(),
            contents: HashMap::new(),
            all_apps,
            start_open: false,
            start_search: String::new(),
            selected_icon: None,
            status: None,
            session: None,
            last_poll: Instant::now() - Duration::from_secs(10),
            started: Instant::now(),
        };
        shell.open_builtin("about", "Sobre o LuwiDE", "ℹ");
        shell
    }

    /* ------------------------- abertura de apps ------------------------- */

    fn open_builtin(&mut self, id: &str, title: &str, icon: &str) -> Option<u64> {
        let (title, icon, size) = match id {
            "explorer" => ("Explorer", "🗂", (700.0, 450.0)),
            "terminal" => ("Terminal", "💻", (640.0, 410.0)),
            "notepad" => ("Bloco de Notas", "📝", (580.0, 430.0)),
            "calculator" => ("Calculadora", "🧮", (270.0, 370.0)),
            "settings" => ("Configurações", "⚙", (540.0, 440.0)),
            "about" => ("Sobre o LuwiDE", "ℹ", (480.0, 380.0)),
            "monitor" => ("Monitor do Sistema", "📊", (560.0, 420.0)),
            _ => (title, icon, (560.0, 400.0)),
        };
        let wid = self.wm.open(id, title, icon, size);
        if !self.contents.contains_key(&wid) {
            let content = match id {
                "explorer" => AppContent::Explorer(ExplorerState::new(luwide_common::home_dir())),
                "terminal" => AppContent::Terminal(TerminalState::new(luwide_common::home_dir())),
                "notepad" => AppContent::Notepad(NotepadState::new()),
                "calculator" => AppContent::Calculator(Calculator::new()),
                "settings" => AppContent::Settings,
                "monitor" => AppContent::Monitor,
                _ => AppContent::About,
            };
            self.contents.insert(wid, content);
        }
        Some(wid)
    }

    fn launch(&mut self, app: &DesktopEntry) {
        if app.builtin {
            let id = app.exec.strip_prefix("builtin:").unwrap_or(&app.id);
            self.open_builtin(id, &app.title, &app.icon);
            return;
        }
        // Aplicativo externo: quem lança e rastreia é o sessiond (se ativo);
        // sem ele, lançamos direto e avisamos na barra de status.
        match luwide_common::ipc::request(&IpcRequest::LaunchApp {
            exec: app.exec.clone(),
            title: app.title.clone(),
        }) {
            Ok(IpcResponse::Launched { app: running }) => {
                self.set_status(format!("{} iniciado (pid {}) via sessiond", app.title, running.pid));
            }
            Ok(IpcResponse::Error { message }) => {
                self.set_status(format!("Erro ao iniciar {}: {message}", app.title));
            }
            _ => match spawn_external(&app.exec) {
                Ok(pid) => {
                    let _ = luwide_common::ipc::request(&IpcRequest::RegisterApp {
                        pid,
                        title: app.title.clone(),
                        exec: app.exec.clone(),
                    });
                    self.set_status(format!("{} iniciado (pid {pid})", app.title));
                }
                Err(e) => self.set_status(format!("Não foi possível iniciar {}: {e}", app.title)),
            },
        }
    }

    fn set_status(&mut self, msg: String) {
        self.status = Some((msg, Instant::now()));
    }

    fn poll_session(&mut self) {
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

    /* ------------------------------ update ------------------------------ */

    fn process_actions(&mut self, ctx: &egui::Context, actions: Vec<Action>, work: RectF) {
        for action in actions {
            match action {
                Action::Close(id) => {
                    self.wm.close(id);
                    self.contents.remove(&id);
                }
                Action::Minimize(id) => self.wm.minimize(id),
                Action::ToggleMax(id) => self.wm.toggle_maximize(id, work),
                Action::Focus(id) => self.wm.focus(id),
                Action::TaskbarClick(id) => self.wm.taskbar_click(id),
                Action::SetRect(id, rect) => self.wm.set_rect(id, rect, work),
                Action::OpenApp(app) => {
                    self.start_open = false;
                    self.launch(&app);
                }
                Action::OpenExplorerAt(path) => {
                    self.start_open = false;
                    if let Some(id) = self.open_builtin("explorer", "Explorer", "🗂") {
                        if let Some(AppContent::Explorer(state)) = self.contents.get_mut(&id) {
                            state.navigate(path);
                        }
                    }
                }
                Action::LaunchExternal { exec, title } => {
                    self.launch(&DesktopEntry {
                        id: exec.clone(),
                        title,
                        exec,
                        icon: "📦".into(),
                        builtin: false,
                    });
                }
                Action::CloseShell => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Action::Status(msg) => self.set_status(msg),
            }
        }
    }
}

fn spawn_external(exec: &str) -> std::io::Result<u32> {
    let mut parts = exec.split_whitespace();
    let program = parts.next().unwrap_or("");
    if program.is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "comando vazio"));
    }
    let child = std::process::Command::new(program)
        .args(parts)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(child.id())
}

fn to_egui(r: RectF) -> Rect {
    Rect::from_min_size(Pos2::new(r.x, r.y), Vec2::new(r.w, r.h))
}
fn to_rectf(r: Rect) -> RectF {
    RectF::new(r.min.x, r.min.y, r.width(), r.height())
}

impl eframe::App for LuwiShell {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(cfg) = &mut self.config {
            let _ = cfg.reload_if_changed();
        }
        self.poll_session();

        let screen = ctx.screen_rect();
        let taskbar_h = self.config.as_ref().map(|c| c.current().panel.height).unwrap_or(40.0);
        let work = RectF::new(0.0, 0.0, screen.width(), (screen.height() - taskbar_h).max(100.0));
        let work_egui = Rect::from_min_size(Pos2::ZERO, Vec2::new(work.w, work.h));
        let mut actions: Vec<Action> = Vec::new();

        egui::CentralPanel::default()
            .frame(egui::Frame::default().inner_margin(egui::Margin::ZERO))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                self.paint_wallpaper(ui.painter(), rect);
                self.desktop_icons(ui, work_egui, &mut actions);
            });

        // Janelas (na ordem de empilhamento)
        for id in self.wm.visible_ids() {
            if let Some(win) = self.wm.get(id).cloned() {
                let active = self.wm.active == Some(id);
                self.render_window(ctx, &win, active, work, &mut actions);
            }
        }

        if self.start_open {
            self.render_start_menu(ctx, screen, taskbar_h, &mut actions);
        }
        self.render_taskbar(ctx, screen, taskbar_h, &mut actions);

        // Toast de status (canto inferior direito, acima da barra)
        if let Some((msg, when)) = &self.status {
            if when.elapsed() < Duration::from_secs(5) {
                let pos = Pos2::new(screen.max.x - 12.0, screen.max.y - taskbar_h - 12.0);
                egui::Area::new(Id::new("luwi-toast"))
                    .fixed_pos(pos)
                    .order(Order::Foreground)
                    .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::ZERO)
                    .show(ctx, |ui| {
                        egui::Frame::default()
                            .fill(Color32::from_rgb(255, 251, 220))
                            .stroke(Stroke::new(1.0, XpTheme::WINDOW_EDGE))
                            .rounding(Rounding::same(4.0))
                            .inner_margin(egui::Margin::same(8.0))
                            .show(ui, |ui| {
                                ui.label(RichText::new(msg).color(XpTheme::TEXT).size(12.5));
                            });
                    });
            }
        }

        self.process_actions(ctx, actions, work);
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

/* ============================ partes da tela ============================ */

impl LuwiShell {
    fn paint_wallpaper(&self, painter: &egui::Painter, rect: Rect) {
        let style = self.config.as_ref().map(|c| c.current().theme.wallpaper.clone());
        let accent = self.config.as_ref().map(|c| color_from_hex(&c.current().theme.accent));
        match style.as_deref() {
            Some("solid") => {
                painter.rect_filled(rect, Rounding::ZERO, accent.unwrap_or(XpTheme::TASKBAR));
            }
            Some("gradient") => {
                let top = accent.unwrap_or(XpTheme::DESKTOP_SKY_TOP);
                XpTheme::gradient_vertical(&painter, rect, top, XpTheme::DESKTOP_SKY_LIGHT, 24);
            }
            _ => {
                // "Bliss": céu em gradiente, sol, colinas verdes.
                let horizon = rect.top() + rect.height() * 0.68;
                let sky = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, horizon));
                XpTheme::gradient_vertical(
                    painter,
                    sky,
                    XpTheme::DESKTOP_SKY_TOP,
                    XpTheme::DESKTOP_SKY_LIGHT,
                    28,
                );
                let grass = Rect::from_min_max(Pos2::new(rect.min.x, horizon), rect.max);
                XpTheme::gradient_vertical(
                    painter,
                    grass,
                    XpTheme::DESKTOP_GRASS,
                    XpTheme::DESKTOP_GRASS_DARK,
                    18,
                );
                // Sol com brilho
                let sun = Pos2::new(rect.min.x + rect.width() * 0.78, rect.min.y + rect.height() * 0.18);
                painter.circle_filled(sun, 74.0, Color32::from_rgba_unmultiplied(255, 244, 180, 60));
                painter.circle_filled(sun, 44.0, Color32::from_rgb(255, 240, 170));
                // Colinas
                painter.add(egui::Shape::ellipse_filled(
                    Pos2::new(rect.min.x + rect.width() * 0.22, rect.max.y + rect.height() * 0.10),
                    Vec2::new(rect.width() * 0.55, rect.height() * 0.30),
                    XpTheme::DESKTOP_GRASS,
                ));
                painter.add(egui::Shape::ellipse_filled(
                    Pos2::new(rect.min.x + rect.width() * 0.88, rect.max.y + rect.height() * 0.16),
                    Vec2::new(rect.width() * 0.48, rect.height() * 0.26),
                    XpTheme::DESKTOP_GRASS_DARK,
                ));
                // Nuvens simples
                for (fx, fy, s) in [(0.16, 0.14, 1.0), (0.48, 0.09, 0.7), (0.66, 0.26, 0.55)] {
                    let c = Pos2::new(rect.min.x + rect.width() * fx, rect.min.y + rect.height() * fy);
                    let cloud = Color32::from_rgba_unmultiplied(255, 255, 255, 215);
                    painter.circle_filled(c, 22.0 * s, cloud);
                    painter.circle_filled(c + Vec2::new(24.0 * s, 4.0 * s), 17.0 * s, cloud);
                    painter.circle_filled(c + Vec2::new(-24.0 * s, 5.0 * s), 15.0 * s, cloud);
                }
            }
        }
    }

    fn desktop_icons(&mut self, ui: &mut egui::Ui, _work: Rect, actions: &mut Vec<Action>) {
        let apps: Vec<DesktopEntry> = self
            .config
            .as_ref()
            .map(|c| c.current().apps.clone())
            .unwrap_or_else(luwide_common::default_apps);
        let mut pos = Pos2::new(14.0, 14.0);
        for app in apps {
            let rect = Rect::from_min_size(pos, Vec2::new(92.0, 78.0));
            let resp = ui.interact(rect, Id::new(("desktop-icon", &app.id)), Sense::click());
            if self.selected_icon.as_deref() == Some(app.id.as_str()) {
                ui.painter().rect_filled(
                    rect,
                    Rounding::same(4.0),
                    Color32::from_rgba_unmultiplied(255, 255, 255, 45),
                );
            }
            let center = Pos2::new(rect.center().x, rect.top() + 24.0);
            ui.painter().text(
                center + Vec2::new(1.0, 1.0),
                egui::Align2::CENTER_CENTER,
                &app.icon,
                FontId::proportional(28.0),
                Color32::from_rgba_unmultiplied(0, 0, 0, 120),
            );
            ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                &app.icon,
                FontId::proportional(28.0),
                Color32::WHITE,
            );
            // Rótulo com sombra (legível sobre o céu/grama)
            let label_pos = Pos2::new(rect.center().x, rect.top() + 56.0);
            ui.painter().text(
                label_pos + Vec2::new(1.0, 1.0),
                egui::Align2::CENTER_CENTER,
                &app.title,
                FontId::proportional(11.5),
                Color32::from_rgba_unmultiplied(0, 0, 40, 180),
            );
            ui.painter().text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                &app.title,
                FontId::proportional(11.5),
                Color32::WHITE,
            );
            if resp.clicked() {
                self.selected_icon = Some(app.id.clone());
            }
            if resp.double_clicked() {
                actions.push(Action::OpenApp(app.clone()));
            }
            pos.y += 88.0;
        }
        if ui.input(|i| i.pointer.primary_clicked()) {
            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                if p.x > 130.0 {
                    self.selected_icon = None;
                }
            }
        }
    }

    fn render_taskbar(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        height: f32,
        actions: &mut Vec<Action>,
    ) {
        let bar = Rect::from_min_max(
            Pos2::new(screen.min.x, screen.max.y - height),
            screen.max,
        );
        egui::Area::new(Id::new("luwi-taskbar"))
            .fixed_pos(bar.min)
            .order(Order::Foreground)
            .show(ctx, |ui| {
                let (_, full) = ui.allocate_space(bar.size());
                let painter = ui.painter().clone();
                XpTheme::gradient_vertical(&painter, full, XpTheme::TASKBAR_HIGHLIGHT, XpTheme::TASKBAR, 10);
                painter.line_segment(
                    [full.left_top(), full.right_top()],
                    Stroke::new(2.0, Color32::from_rgb(140, 190, 255)),
                );

                // Botão Iniciar
                let label = self
                    .config
                    .as_ref()
                    .map(|c| c.current().panel.start_label.clone())
                    .unwrap_or_else(|| "Iniciar".into());
                let start_rect = Rect::from_min_size(
                    Pos2::new(full.min.x + 2.0, full.min.y + 3.0),
                    Vec2::new(118.0, height - 6.0),
                );
                let start_resp = ui.interact(start_rect, Id::new("start-button"), Sense::click());
                let start_color = if self.start_open { XpTheme::START_BUTTON_EDGE } else { XpTheme::START_BUTTON };
                XpTheme::gradient_vertical(&painter, start_rect, XpTheme::START_BUTTON_LIGHT, start_color, 8);
                painter.rect_stroke(start_rect, Rounding::same(6.0), Stroke::new(1.0, XpTheme::START_BUTTON_EDGE));
                painter.text(
                    start_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("🏁 {label}"),
                    FontId::proportional(15.0),
                    Color32::WHITE,
                );
                if start_resp.clicked() {
                    self.start_open = !self.start_open;
                }

                // Botões das janelas
                let mut x = start_rect.max.x + 8.0;
                let windows = self.wm.windows.clone();
                for win in &windows {
                    if x + 150.0 > full.max.x - 190.0 {
                        break;
                    }
                    let rect = Rect::from_min_size(Pos2::new(x, full.min.y + 5.0), Vec2::new(150.0, height - 10.0));
                    let resp = ui.interact(rect, Id::new(("taskbtn", win.id)), Sense::click());
                    let active = self.wm.active == Some(win.id) && !win.minimized;
                    let fill = if active {
                        XpTheme::TASKBAR_DARK
                    } else {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 38)
                    };
                    painter.rect_filled(rect, Rounding::same(3.0), fill);
                    painter.text(
                        Pos2::new(rect.min.x + 8.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        format!("{} {}", win.icon, truncate(&win.title, 16)),
                        FontId::proportional(12.0),
                        Color32::WHITE,
                    );
                    if resp.clicked() {
                        actions.push(Action::TaskbarClick(win.id));
                    }
                    x += 156.0;
                }

                // Apps externos (via sessiond)
                if let Some(session) = &self.session {
                    for app in &session.running_apps {
                        if x + 130.0 > full.max.x - 190.0 {
                            break;
                        }
                        let rect = Rect::from_min_size(Pos2::new(x, full.min.y + 5.0), Vec2::new(130.0, height - 10.0));
                        painter.rect_filled(
                            rect,
                            Rounding::same(3.0),
                            Color32::from_rgba_unmultiplied(0, 0, 0, 45),
                        );
                        painter.text(
                            Pos2::new(rect.min.x + 8.0, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            format!("▶ {}", truncate(&app.title, 13)),
                            FontId::proportional(11.5),
                            Color32::WHITE,
                        );
                        x += 136.0;
                    }
                }

                // Bandeja (tray) com relógio
                let show_clock = self.config.as_ref().map(|c| c.current().panel.show_clock).unwrap_or(true);
                let tray_w = if show_clock { 178.0 } else { 108.0 };
                let tray = Rect::from_min_max(
                    Pos2::new(full.max.x - tray_w, full.min.y + 3.0),
                    Pos2::new(full.max.x - 3.0, full.max.y - 3.0),
                );
                painter.rect_filled(tray, Rounding::same(4.0), XpTheme::TRAY_BG);
                let session_dot = if self.session.is_some() { "🟢" } else { "⚪" };
                let mut text = format!("{session_dot} 🔊 📶 🔋");
                if show_clock {
                    text.push_str(&format!("  {}", luwide_common::now_clock()));
                }
                let tray_resp = ui.interact(tray, Id::new("tray"), Sense::click());
                painter.text(tray.center(), egui::Align2::CENTER_CENTER, text, FontId::proportional(12.5), Color32::WHITE);
                if tray_resp.clicked() {
                    if let Some(app) = self.all_apps.iter().find(|a| a.id == "monitor").cloned() {
                        let _ = app;
                    }
                    actions.push(Action::OpenApp(DesktopEntry {
                        id: "monitor".into(),
                        title: "Monitor do Sistema".into(),
                        exec: "builtin:monitor".into(),
                        icon: "📊".into(),
                        builtin: true,
                    }));
                }
            });
    }

    fn render_start_menu(
        &mut self,
        ctx: &egui::Context,
        screen: Rect,
        taskbar_h: f32,
        actions: &mut Vec<Action>,
    ) {
        let menu = Rect::from_min_max(
            Pos2::new(screen.min.x + 2.0, screen.max.y - taskbar_h - 500.0),
            Pos2::new(screen.min.x + 402.0, screen.max.y - taskbar_h + 2.0),
        );
        egui::Area::new(Id::new("luwi-start-menu"))
            .fixed_pos(menu.min)
            .order(Order::Foreground)
            .show(ctx, |ui| {
                let (_, full) = ui.allocate_space(menu.size());
                let painter = ui.painter().clone();
                painter.rect_filled(full, Rounding::same(6.0), XpTheme::WINDOW_BG);
                painter.rect_stroke(full, Rounding::same(6.0), Stroke::new(1.5, XpTheme::WINDOW_EDGE));

                // Cabeçalho azul com o usuário
                let header = Rect::from_min_max(full.min, Pos2::new(full.max.x, full.min.y + 58.0));
                XpTheme::gradient_vertical(&painter, header, XpTheme::TASKBAR_HIGHLIGHT, XpTheme::TASKBAR, 10);
                let user = self
                    .config
                    .as_ref()
                    .map(|c| c.current().theme.user_name.clone())
                    .unwrap_or_else(|| "Usuário".into());
                painter.text(
                    Pos2::new(header.min.x + 16.0, header.center().y),
                    egui::Align2::LEFT_CENTER,
                    format!("👤 {user}"),
                    FontId::proportional(17.0),
                    Color32::WHITE,
                );

                // Busca
                let search_rect = Rect::from_min_size(
                    Pos2::new(full.min.x + 10.0, header.max.y + 8.0),
                    Vec2::new(full.width() - 20.0, 24.0),
                );
                ui.allocate_ui_at_rect(search_rect, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.start_search)
                            .hint_text("🔎 Buscar programa…")
                            .desired_width(f32::INFINITY),
                    );
                });

                // Lista de programas (rolável)
                let list_rect = Rect::from_min_max(
                    Pos2::new(full.min.x + 6.0, search_rect.max.y + 6.0),
                    Pos2::new(full.max.x - 6.0, full.max.y - 46.0),
                );
                let query = self.start_search.to_lowercase();
                let apps: Vec<DesktopEntry> = self
                    .all_apps
                    .iter()
                    .filter(|a| query.is_empty() || a.title.to_lowercase().contains(&query))
                    .take(14)
                    .cloned()
                    .collect();
                ui.allocate_ui_at_rect(list_rect, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.set_width(list_rect.width());
                        for app in &apps {
                            let resp = ui.add(
                                egui::Button::new(
                                    RichText::new(format!("{}  {}", app.icon, app.title)).size(13.5),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::NONE),
                            );
                            if resp.clicked() {
                                actions.push(Action::OpenApp(app.clone()));
                            }
                        }
                        if apps.is_empty() {
                            ui.label(RichText::new("Nenhum programa encontrado.").color(XpTheme::TEXT_MUTED));
                        }
                    });
                });

                // Rodapé: locais + desligar
                let footer = Rect::from_min_max(Pos2::new(full.min.x, full.max.y - 42.0), full.max);
                XpTheme::gradient_vertical(&painter, footer, XpTheme::TASKBAR, XpTheme::TASKBAR_DARK, 8);
                let home = luwide_common::home_dir();
                let places: Vec<(&str, Action)> = vec![
                    ("🗂 Pasta pessoal", Action::OpenExplorerAt(home.clone())),
                    ("📄 Documentos", Action::OpenExplorerAt(home.join("Documents"))),
                ];
                let mut x = footer.min.x + 8.0;
                for (label, action) in places {
                    let rect = Rect::from_min_size(Pos2::new(x, footer.min.y + 7.0), Vec2::new(118.0, 28.0));
                    if ui.interact(rect, Id::new(("place", label)), Sense::click()).clicked() {
                        actions.push(action);
                    }
                    painter.text(
                        Pos2::new(rect.min.x + 4.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        label,
                        FontId::proportional(11.5),
                        Color32::WHITE,
                    );
                    x += 122.0;
                }
                let off_rect = Rect::from_min_size(
                    Pos2::new(footer.max.x - 118.0, footer.min.y + 7.0),
                    Vec2::new(110.0, 28.0),
                );
                painter.rect_filled(off_rect, Rounding::same(4.0), XpTheme::CLOSE_BUTTON);
                painter.text(off_rect.center(), egui::Align2::CENTER_CENTER, "⏻ Desligar", FontId::proportional(12.0), Color32::WHITE);
                if ui.interact(off_rect, Id::new("power-off"), Sense::click()).clicked() {
                    actions.push(Action::CloseShell);
                }
            });

        // Fecha ao clicar fora do menu
        if ctx.input(|i| i.pointer.primary_clicked()) {
            if let Some(p) = ctx.input(|i| i.pointer.interact_pos()) {
                if !menu.contains(p) && p.y < screen.max.y - taskbar_h {
                    self.start_open = false;
                }
            }
        }
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(max - 1).collect();
        s.push('…');
        s
    }
}

/* ============================ janelas e apps ============================ */

impl LuwiShell {
    fn render_window(
        &mut self,
        ctx: &egui::Context,
        win: &crate::wm::WindowState,
        active: bool,
        work: RectF,
        actions: &mut Vec<Action>,
    ) {
        let rect = if win.maximized { to_egui(work) } else { to_egui(win.rect) };
        let id = win.id;
        egui::Area::new(Id::new(("luwi-window", id)))
            .fixed_pos(rect.min)
            .order(if active { Order::Foreground } else { Order::Middle })
            .show(ctx, |ui| {
                let (_, full) = ui.allocate_space(rect.size());
                let painter = ui.painter().clone();
                let edge = if active { XpTheme::WINDOW_EDGE } else { XpTheme::WINDOW_EDGE_INACTIVE };
                painter.rect_filled(full, Rounding::same(6.0), XpTheme::WINDOW_BG);
                painter.rect_stroke(full, Rounding::same(6.0), Stroke::new(1.5, edge));

                // Barra de título
                let header = Rect::from_min_max(full.min, Pos2::new(full.max.x, full.min.y + 30.0));
                let (top, bottom) = if active {
                    (XpTheme::TASKBAR_HIGHLIGHT, XpTheme::TITLE_BAR)
                } else {
                    (XpTheme::TITLE_BAR_INACTIVE, XpTheme::TITLE_BAR_INACTIVE)
                };
                // Recorta o gradiente dentro do retângulo da janela
                painter.with_clip_rect(Rect::from_min_max(full.min, Pos2::new(full.max.x, full.min.y + 30.0)));
                XpTheme::gradient_vertical(&painter, header, top, bottom, 8);
                painter.text(
                    Pos2::new(header.min.x + 10.0, header.center().y),
                    egui::Align2::LEFT_CENTER,
                    format!("{}  {}", win.icon, win.title),
                    FontId::proportional(13.5),
                    XpTheme::TITLE_TEXT,
                );

                // Arrastar pela barra de título
                let buttons_w = 3.0 * 28.0 + 8.0;
                let drag_rect = Rect::from_min_max(
                    header.min,
                    Pos2::new(header.max.x - buttons_w, header.max.y),
                );
                let drag_resp = ui.interact(drag_rect, Id::new(("win-drag", id)), Sense::click_and_drag());
                if drag_resp.clicked() {
                    actions.push(Action::Focus(id));
                }
                if drag_resp.dragged() && !win.maximized {
                    let d = drag_resp.drag_delta();
                    let mut r = win.rect;
                    r.x += d.x;
                    r.y += d.y;
                    actions.push(Action::SetRect(id, r));
                    actions.push(Action::Focus(id));
                }

                // Botões da janela: minimizar, maximizar, fechar
                let defs = [
                    ("min", "—", XpTheme::WINDOW_BUTTON),
                    ("max", if win.maximized { "🗗" } else { "▢" }, XpTheme::WINDOW_BUTTON),
                    ("close", "✕", XpTheme::CLOSE_BUTTON),
                ];
                for (i, (name, glyph, color)) in defs.iter().enumerate() {
                    let btn = Rect::from_min_size(
                        Pos2::new(header.max.x - buttons_w + 4.0 + i as f32 * 28.0, header.min.y + 5.0),
                        Vec2::new(24.0, 20.0),
                    );
                    let resp = ui.interact(btn, Id::new(("win-btn", id, name)), Sense::click());
                    painter.rect_filled(btn, Rounding::same(3.0), *color);
                    painter.text(btn.center(), egui::Align2::CENTER_CENTER, *glyph, FontId::proportional(11.0), Color32::WHITE);
                    if resp.clicked() {
                        match *name {
                            "min" => actions.push(Action::Minimize(id)),
                            "max" => actions.push(Action::ToggleMax(id)),
                            _ => actions.push(Action::Close(id)),
                        }
                    }
                }

                // Conteúdo
                let content_rect = Rect::from_min_max(
                    Pos2::new(full.min.x + 6.0, header.max.y + 6.0),
                    Pos2::new(full.max.x - 6.0, full.max.y - 6.0),
                );
                ui.allocate_ui_at_rect(content_rect, |ui| {
                    ui.set_width(content_rect.width());
                    ui.set_height(content_rect.height());
                    self.render_content(ui, id, content_rect, actions);
                });

                // Alça de redimensionamento (canto inferior direito)
                if !win.maximized {
                    let handle_rect = Rect::from_min_max(
                        Pos2::new(full.max.x - 18.0, full.max.y - 18.0),
                        full.max,
                    );
                    let resp = ui.interact(handle_rect, Id::new(("win-resize", id)), Sense::click_and_drag());
                    painter.text(handle_rect.center(), egui::Align2::CENTER_CENTER, "◢", FontId::proportional(10.0), XpTheme::TEXT_MUTED);
                    if resp.dragged() {
                        let d = resp.drag_delta();
                        let mut r = win.rect;
                        r.w += d.x;
                        r.h += d.y;
                        actions.push(Action::SetRect(id, r));
                    }
                }
            });
    }

    fn render_content(
        &mut self,
        ui: &mut egui::Ui,
        id: u64,
        rect: Rect,
        actions: &mut Vec<Action>,
    ) {
        let Some(content) = self.contents.get_mut(&id) else { return };
        match content {
            AppContent::Explorer(state) => {
                ui.horizontal(|ui| {
                    if ui.button("⬆ Subir").clicked() {
                        state.up();
                    }
                    if ui.button("🏠 Início").clicked() {
                        state.navigate(luwide_common::home_dir());
                    }
                    ui.label(RichText::new(state.path.display().to_string()).strong());
                });
                if let Some(err) = &state.error {
                    ui.label(RichText::new(err).color(Color32::from_rgb(190, 40, 30)));
                }
                ui.separator();
                let items = state.list();
                let mut navigate: Option<PathBuf> = None;
                let mut open_file: Option<PathBuf> = None;
                egui::ScrollArea::vertical().max_height(rect.height() - 64.0).show(ui, |ui| {
                    ui.set_width(rect.width() - 12.0);
                    if items.is_empty() {
                        ui.label(RichText::new("(pasta vazia ou sem permissão)").color(XpTheme::TEXT_MUTED));
                    }
                    for item in &items {
                        ui.horizontal(|ui| {
                            let icon = if item.is_dir { "📁" } else { "📄" };
                            let resp = ui.add(
                                egui::Button::new(format!("{icon} {}", item.name))
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(Stroke::NONE),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if !item.is_dir {
                                    ui.label(
                                        RichText::new(crate::apps::human_size(item.size))
                                            .color(XpTheme::TEXT_MUTED)
                                            .size(11.0),
                                    );
                                }
                            });
                            if resp.double_clicked() {
                                if item.is_dir {
                                    navigate = Some(item.path.clone());
                                } else {
                                    open_file = Some(item.path.clone());
                                }
                            }
                        });
                    }
                });
                ui.separator();
                ui.label(
                    RichText::new(format!("{} itens — dois cliques para abrir", items.len()))
                        .color(XpTheme::TEXT_MUTED)
                        .size(11.0),
                );
                if let Some(path) = navigate {
                    state.navigate(path);
                }
                if let Some(path) = open_file {
                    actions.push(Action::LaunchExternal {
                        exec: format!("xdg-open {}", path.display()),
                        title: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                    });
                }
            }
            AppContent::Terminal(state) => {
                egui::ScrollArea::vertical()
                    .max_height(rect.height() - 40.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.set_width(rect.width() - 12.0);
                        for line in &state.lines {
                            ui.label(RichText::new(line).font(FontId::monospace(12.0)).color(XpTheme::TEXT));
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("❯").strong());
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut state.input)
                            .font(FontId::monospace(12.5))
                            .desired_width(rect.width() - 40.0)
                            .hint_text("digite um comando e Enter"),
                    );
                    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if enter {
                        match state.run_input() {
                            TermAction::Exit => actions.push(Action::Close(id)),
                            _ => {}
                        }
                        resp.request_focus();
                    }
                });
            }
            AppContent::Notepad(state) => {
                ui.horizontal(|ui| {
                    ui.label("Arquivo:");
                    ui.add(egui::TextEdit::singleline(&mut state.path_text).desired_width(rect.width() - 260.0));
                    if ui.button("📂 Abrir").clicked() {
                        state.open();
                    }
                    if ui.button("💾 Salvar").clicked() {
                        state.save();
                    }
                    if ui.button("🆕 Novo").clicked() {
                        *state = NotepadState::new();
                    }
                });
                ui.separator();
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut state.text)
                        .font(FontId::monospace(12.5))
                        .desired_width(rect.width() - 14.0)
                        .desired_rows(18),
                );
                if resp.changed() {
                    state.dirty = true;
                }
                ui.label(
                    RichText::new(format!("{}{}", state.status, if state.dirty { " • (alterado)" } else { "" }))
                        .color(XpTheme::TEXT_MUTED)
                        .size(11.0),
                );
            }
            AppContent::Calculator(calc) => {
                egui::Frame::default()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1.0, XpTheme::WINDOW_EDGE_INACTIVE))
                    .inner_margin(egui::Margin::same(8.0))
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(calc.display()).size(24.0).strong());
                        });
                    });
                ui.add_space(6.0);
                let rows: [[&str; 4]; 5] = [
                    ["C", "B", "/", "*"],
                    ["7", "8", "9", "-"],
                    ["4", "5", "6", "+"],
                    ["1", "2", "3", "="],
                    ["0", ".", "(", ")"],
                ];
                for row in rows {
                    ui.horizontal(|ui| {
                        for key in row {
                            let label = match key {
                                "B" => "⌫",
                                _ => key,
                            };
                            if ui.add_sized([52.0, 38.0], egui::Button::new(RichText::new(label).size(15.0))).clicked() {
                                if key == "(" || key == ")" {
                                    // Parênteses ainda não são suportados pela
                                    // lógica simples da calculadora.
                                } else {
                                    calc.press(key.chars().next().unwrap_or(' '));
                                }
                            }
                        }
                    });
                    ui.add_space(3.0);
                }
            }
            AppContent::Settings => unreachable!("tratado acima"),
            AppContent::About => {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("🪟 LuwiDE").size(30.0).strong().color(XpTheme::WINDOW_EDGE));
                    ui.label(RichText::new("Ambiente de Trabalho leve, em Rust, com visual XP nostálgico").color(XpTheme::TEXT_MUTED));
                });
                ui.add_space(8.0);
                ui.separator();
                let cfg_path = self
                    .config
                    .as_ref()
                    .map(|c| c.path().display().to_string())
                    .unwrap_or_default();
                for (k, v) in [
                    ("Versão", env!("CARGO_PKG_VERSION")),
                    ("Componentes", "compositor Wayland • shell • panel • launcher • sessiond"),
                    ("Configuração", cfg_path.as_str()),
                    ("Orçamento de memória", "200 MB no total (compositor 70 / shell 60 / panel 20 / launcher 25 / sessiond 25)"),
                    ("Sessiond", if self.session.is_some() { "conectado 🟢" } else { "não detectado (modo degradado)" }),
                    ("Futuro", "shell padrão do LuwiOS (Linux em Rust)"),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{k}: ")).strong());
                        ui.label(v);
                    });
                }
                ui.add_space(6.0);
                ui.label("Dica: abra o Terminal e digite 'help'. Arraste as janelas pela barra azul, redimensione pelo canto inferior direito e use o menu Iniciar (canto inferior esquerdo).");
            }
            AppContent::Monitor => {
                let rss = luwide_common::memory::current_rss_mb().unwrap_or(0);
                ui.heading("Monitor do Sistema");
                ui.separator();
                ui.label(format!("Shell (este processo): RSS {rss} MB"));
                ui.label(format!("Janelas abertas: {}", self.wm.windows.len()));
                ui.label(format!("Tempo de sessão da shell: {}s", self.started.elapsed().as_secs()));
                if let Some(snapshot) = self.session.clone() {
                    ui.label(format!("sessiond: RSS {} MB • uptime {}s • relógio {}", snapshot.sessiond_rss_mb, snapshot.uptime_secs, snapshot.clock));
                    ui.label(format!("Workspace focado: {}", snapshot.focused_workspace));
                    ui.separator();
                    ui.label(RichText::new("Aplicativos externos em execução:").strong());
                    if snapshot.running_apps.is_empty() {
                        ui.label(RichText::new("(nenhum)").color(XpTheme::TEXT_MUTED));
                    }
                    for app in &snapshot.running_apps {
                        ui.label(format!("▶ {} (pid {}, workspace {})", app.title, app.pid, app.workspace));
                    }
                } else {
                    ui.label(RichText::new("sessiond não está rodando. Inicie com: cargo run -p luwide-sessiond").color(Color32::from_rgb(170, 90, 20)));
                }
                ui.separator();
                if let Some(cfg) = &self.config {
                    let b = &cfg.current().memory_budget;
                    ui.label(RichText::new(format!("Orçamento total: {} MB", b.total_mb())).strong());
                    for (name, mb) in [
                        ("compositor", b.compositor_mb),
                        ("shell", b.shell_mb),
                        ("panel", b.panel_mb),
                        ("launcher", b.launcher_mb),
                        ("sessiond", b.sessiond_mb),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(format!("{name}: {mb} MB"));
                            ui.add(egui::ProgressBar::new(mb as f32 / b.total_mb() as f32).desired_width(140.0));
                        });
                    }
                }
            }
        }
    }

    fn render_settings(&mut self, ui: &mut egui::Ui, _rect: Rect) {
        ui.heading("Configurações do LuwiDE");
        ui.separator();
        if self.config.is_none() {
            ui.label("Configuração indisponível.");
            return;
        }
        let mut save = false;
        let cfg_path = self.config.as_ref().map(|c| c.path().display().to_string()).unwrap_or_default();
        {
            let cfg = self.config.as_mut().expect("config verificada acima");
            let c = cfg.current_mut();
            ui.horizontal(|ui| {
                ui.label("Usuário:");
                ui.text_edit_singleline(&mut c.theme.user_name);
            });
            ui.horizontal(|ui| {
                ui.label("Rótulo do botão Iniciar:");
                ui.text_edit_singleline(&mut c.panel.start_label);
            });
            ui.checkbox(&mut c.panel.show_clock, "Mostrar relógio na barra de tarefas");
            ui.add_space(6.0);
            ui.label(RichText::new("Papel de parede:").strong());
            for (value, label) in [
                ("bliss", "🌄 Bliss (céu e colina, estilo XP)"),
                ("gradient", "🎨 Gradiente na cor de destaque"),
                ("solid", "⬛ Cor sólida de destaque"),
            ] {
                ui.radio_value(&mut c.theme.wallpaper, value.to_string(), label);
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Cor de destaque:").strong());
            ui.horizontal(|ui| {
                for hex in ["#245edb", "#3a7d2c", "#7d3ac1", "#c13a3a", "#e07b1f", "#128a8a"] {
                    let selected = c.theme.accent.eq_ignore_ascii_case(hex);
                    let fill = color_from_hex(hex);
                    let resp = ui.add(
                        egui::Button::new(if selected { "✓" } else { " " })
                            .fill(fill)
                            .min_size(Vec2::new(30.0, 24.0)),
                    );
                    if resp.clicked() {
                        c.theme.accent = hex.to_string();
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(format!("Arquivo: {cfg_path}"));
            save = ui.button("💾 Salvar configurações").clicked();
        }
        if save {
            let result = self.config.as_mut().map(|cfg| cfg.save());
            match result {
                Some(Ok(())) => self.set_status("Configurações salvas em luwide.toml".into()),
                Some(Err(e)) => self.set_status(format!("Erro ao salvar: {e}")),
                None => {}
            }
        }
    }
}
