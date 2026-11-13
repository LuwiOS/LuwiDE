//! Gerenciador de janelas do LuwiDE (lógica pura, sem egui).
//!
//! Mantém a pilha de janelas (ordem = z-order), foco, minimizar,
//! maximizar/restaurar e a geometria de cada janela. A renderização
//! fica em `shell.rs`; aqui tudo é testável sem interface gráfica.

/// Retângulo simples em coordenadas da tela (pixels lógicos).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectF {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowState {
    pub id: u64,
    pub app_id: String,
    pub title: String,
    pub icon: String,
    pub rect: RectF,
    pub minimized: bool,
    pub maximized: bool,
    prev_rect: Option<RectF>,
}

#[derive(Debug, Default)]
pub struct WindowManager {
    /// Ordem do vetor = ordem de empilhamento (último = topo).
    pub windows: Vec<WindowState>,
    pub active: Option<u64>,
    next_id: u64,
    cascade: u32,
}

impl WindowManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Abre uma janela para `app_id`. Se já houver uma janela desse app,
    /// apenas a foca e restaura (instância única por aplicativo interno).
    pub fn open(&mut self, app_id: &str, title: &str, icon: &str, default_size: (f32, f32)) -> u64 {
        if let Some(w) = self.windows.iter().find(|w| w.app_id == app_id) {
            let id = w.id;
            self.focus(id);
            if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
                w.minimized = false;
            }
            return id;
        }
        self.next_id += 1;
        self.cascade = (self.cascade + 1) % 8;
        let offset = 28.0 * self.cascade as f32;
        let id = self.next_id;
        self.windows.push(WindowState {
            id,
            app_id: app_id.to_string(),
            title: title.to_string(),
            icon: icon.to_string(),
            rect: RectF::new(90.0 + offset, 48.0 + offset, default_size.0, default_size.1),
            minimized: false,
            maximized: false,
            prev_rect: None,
        });
        self.active = Some(id);
        id
    }

    pub fn close(&mut self, id: u64) {
        self.windows.retain(|w| w.id != id);
        if self.active == Some(id) {
            self.active = self.windows.last().map(|w| w.id);
        }
    }

    /// Traz a janela para o topo e a torna ativa.
    pub fn focus(&mut self, id: u64) {
        if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
            let w = self.windows.remove(pos);
            self.windows.push(w);
            self.active = Some(id);
        }
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut WindowState> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    pub fn get(&self, id: u64) -> Option<&WindowState> {
        self.windows.iter().find(|w| w.id == id)
    }

    pub fn minimize(&mut self, id: u64) {
        if let Some(w) = self.get_mut(id) {
            w.minimized = true;
        }
        if self.active == Some(id) {
            self.active = self.windows.iter().rev().find(|w| !w.minimized).map(|w| w.id);
        }
    }

    /// Clique num botão da barra de tarefas: minimiza se ativa, senão foca/restaura.
    pub fn taskbar_click(&mut self, id: u64) {
        let is_active = self.active == Some(id);
        let minimized = self.get(id).map(|w| w.minimized).unwrap_or(false);
        if is_active && !minimized {
            self.minimize(id);
        } else {
            if let Some(w) = self.get_mut(id) {
                w.minimized = false;
            }
            self.focus(id);
        }
    }

    pub fn toggle_maximize(&mut self, id: u64, viewport: RectF) {
        if let Some(w) = self.get_mut(id) {
            if w.maximized {
                if let Some(prev) = w.prev_rect.take() {
                    w.rect = prev;
                }
                w.maximized = false;
            } else {
                w.prev_rect = Some(w.rect);
                w.rect = viewport;
                w.maximized = true;
            }
        }
    }

    /// Move/redimensiona mantendo a janela dentro da área útil.
    pub fn set_rect(&mut self, id: u64, rect: RectF, viewport: RectF) {
        if let Some(w) = self.get_mut(id) {
            if w.maximized {
                return;
            }
            let w_min = 180.0;
            let h_min = 120.0;
            w.rect = RectF::new(
                rect.x.clamp(viewport.x - rect.w + 80.0, viewport.right() - 80.0),
                rect.y.clamp(viewport.y, viewport.bottom() - 40.0),
                rect.w.max(w_min),
                rect.h.max(h_min),
            );
        }
    }

    pub fn visible_ids(&self) -> Vec<u64> {
        self.windows.iter().filter(|w| !w.minimized).map(|w| w.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp() -> RectF {
        RectF::new(0.0, 0.0, 1280.0, 680.0)
    }

    #[test]
    fn abrir_foca_e_instancia_unica() {
        let mut wm = WindowManager::new();
        let a = wm.open("terminal", "Terminal", "💻", (560.0, 380.0));
        let b = wm.open("explorer", "Explorer", "🗂", (640.0, 420.0));
        assert_ne!(a, b);
        assert_eq!(wm.active, Some(b));
        // Reabrir o mesmo app não duplica a janela.
        let a2 = wm.open("terminal", "Terminal", "💻", (560.0, 380.0));
        assert_eq!(a2, a);
        assert_eq!(wm.windows.len(), 2);
        assert_eq!(wm.active, Some(a));
    }

    #[test]
    fn minimizar_fechar_e_taskbar() {
        let mut wm = WindowManager::new();
        let a = wm.open("a", "A", "a", (300.0, 200.0));
        let b = wm.open("b", "B", "b", (300.0, 200.0));
        wm.taskbar_click(b); // ativa -> minimiza
        assert!(wm.get(b).unwrap().minimized);
        assert_eq!(wm.active, Some(a));
        wm.taskbar_click(b); // minimizada -> restaura e foca
        assert!(!wm.get(b).unwrap().minimized);
        assert_eq!(wm.active, Some(b));
        wm.close(b);
        assert_eq!(wm.windows.len(), 1);
        assert_eq!(wm.active, Some(a));
        assert_eq!(wm.visible_ids(), vec![a]);
    }

    #[test]
    fn maximizar_e_restaurar() {
        let mut wm = WindowManager::new();
        let a = wm.open("a", "A", "a", (300.0, 200.0));
        let original = wm.get(a).unwrap().rect;
        wm.toggle_maximize(a, vp());
        assert!(wm.get(a).unwrap().maximized);
        assert_eq!(wm.get(a).unwrap().rect, vp());
        wm.toggle_maximize(a, vp());
        assert!(!wm.get(a).unwrap().maximized);
        assert_eq!(wm.get(a).unwrap().rect, original);
    }

    #[test]
    fn rect_permanece_na_tela() {
        let mut wm = WindowManager::new();
        let a = wm.open("a", "A", "a", (300.0, 200.0));
        wm.set_rect(a, RectF::new(5000.0, -50.0, 10.0, 10.0), vp());
        let r = wm.get(a).unwrap().rect;
        assert!(r.x <= vp().right() - 80.0);
        assert!(r.y >= 0.0);
        assert!(r.w >= 180.0 && r.h >= 120.0);
    }
}
