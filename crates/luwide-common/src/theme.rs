//! Tema visual "XP nostálgico" do LuwiDE, centralizado.
//!
//! Paleta inspirada no Luna (azul/verde clássicos), com helpers para
//! pintar gradientes e aplicar o estilo global do egui. Todos os
//! componentes usam estas constantes — nada de cores soltas pela UI.

use egui::{Color32, Context, Rounding, Stroke, Visuals};

pub struct XpTheme;

impl XpTheme {
    // Barra de tarefas / janelas
    pub const TASKBAR: Color32 = Color32::from_rgb(36, 93, 219);
    pub const TASKBAR_DARK: Color32 = Color32::from_rgb(24, 64, 165);
    pub const TASKBAR_HIGHLIGHT: Color32 = Color32::from_rgb(92, 148, 235);
    pub const TITLE_BAR: Color32 = Color32::from_rgb(36, 93, 219);
    pub const TITLE_BAR_INACTIVE: Color32 = Color32::from_rgb(118, 143, 196);
    pub const TITLE_TEXT: Color32 = Color32::WHITE;
    // Botão Iniciar
    pub const START_BUTTON: Color32 = Color32::from_rgb(62, 168, 62);
    pub const START_BUTTON_LIGHT: Color32 = Color32::from_rgb(106, 204, 92);
    pub const START_BUTTON_EDGE: Color32 = Color32::from_rgb(27, 122, 36);
    // Janelas
    pub const WINDOW_BG: Color32 = Color32::from_rgb(236, 241, 250);
    pub const WINDOW_EDGE: Color32 = Color32::from_rgb(30, 80, 190);
    pub const WINDOW_EDGE_INACTIVE: Color32 = Color32::from_rgb(140, 160, 200);
    pub const TEXT: Color32 = Color32::from_rgb(20, 28, 44);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(90, 100, 120);
    // Área de trabalho (wallpaper "Bliss")
    pub const DESKTOP_SKY_TOP: Color32 = Color32::from_rgb(28, 101, 204);
    pub const DESKTOP_SKY: Color32 = Color32::from_rgb(58, 142, 232);
    pub const DESKTOP_SKY_LIGHT: Color32 = Color32::from_rgb(140, 196, 250);
    pub const DESKTOP_GRASS: Color32 = Color32::from_rgb(74, 168, 84);
    pub const DESKTOP_GRASS_DARK: Color32 = Color32::from_rgb(52, 138, 62);
    // Bandeja (tray) e botões de janela
    pub const TRAY_BG: Color32 = Color32::from_rgb(18, 144, 210);
    pub const CLOSE_BUTTON: Color32 = Color32::from_rgb(224, 84, 60);
    pub const WINDOW_BUTTON: Color32 = Color32::from_rgb(52, 116, 224);
    pub const BUTTON_FACE: Color32 = Color32::from_rgb(240, 240, 234);
    pub const SELECTION: Color32 = Color32::from_rgb(49, 106, 197);

    /// Pinta um gradiente vertical em `steps` faixas (barato e sem texturas).
    pub fn gradient_vertical(
        painter: &egui::Painter,
        rect: egui::Rect,
        top: Color32,
        bottom: Color32,
        steps: usize,
    ) {
        let steps = steps.max(1);
        let h = rect.height() / steps as f32;
        for i in 0..steps {
            let t = i as f32 / steps as f32;
            let color = lerp_color(top, bottom, t);
            let strip = egui::Rect::from_min_max(
                egui::pos2(rect.min.x, rect.min.y + h * i as f32),
                egui::pos2(rect.max.x, rect.min.y + h * (i as f32 + 1.0) + 1.0),
            );
            painter.rect_filled(strip, Rounding::ZERO, color);
        }
    }

    /// Aplica o estilo XP global (cores de widgets, seleções, janelas).
    pub fn apply(ctx: &Context) {
        let mut visuals = Visuals::light();
        visuals.window_fill = Self::WINDOW_BG;
        visuals.panel_fill = Self::WINDOW_BG;
        visuals.window_stroke = Stroke::new(1.0, Self::WINDOW_EDGE);
        visuals.selection.bg_fill = Self::SELECTION;
        visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        visuals.widgets.inactive.bg_fill = Self::BUTTON_FACE;
        visuals.widgets.inactive.weak_bg_fill = Self::BUTTON_FACE;
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(220, 232, 250);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(220, 232, 250);
        visuals.widgets.active.bg_fill = Color32::from_rgb(196, 216, 246);
        visuals.override_text_color = Some(Self::TEXT);
        ctx.set_visuals(visuals);
    }
}

/// Interpola duas cores (para gradientes pintados por faixas).
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| -> u8 { (x as f32 + (y as f32 - x as f32) * t).round() as u8 };
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// Converte "#2563be" em `Color32` (com fallback para o azul XP).
pub fn color_from_hex(hex: &str) -> Color32 {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color32::from_rgb(r, g, b);
        }
    }
    XpTheme::TASKBAR
}
