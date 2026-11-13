//! Tipos compartilhados do LuwiDE: configuração versionada, sessão e descoberta de apps.
//!
//! Todos os componentes (shell, panel, launcher, sessiond e compositor) falam
//! através dos tipos definidos aqui, para que o LuwiOS possa reaproveitá-los.

pub mod discovery;
pub mod ipc;
pub mod memory;
pub mod theme;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, time::SystemTime};

/// Um aplicativo lançável (vindo da config ou de um arquivo `.desktop`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesktopEntry {
    pub id: String,
    pub title: String,
    /// Comando a executar (programa + argumentos, sem códigos de campo `%f`/`%u`).
    pub exec: String,
    pub icon: String,
    /// `true` para os aplicativos internos do LuwiDE (explorer, terminal, ...).
    #[serde(default)]
    pub builtin: bool,
}

/// Um aplicativo externo em execução, rastreado pelo `luwide-sessiond`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunningApp {
    pub pid: u32,
    pub title: String,
    pub exec: String,
    pub workspace: u8,
    pub started_unix: u64,
}

/// Retrato do estado da sessão, servido pelo `luwide-sessiond` via IPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub focused_workspace: u8,
    pub running_apps: Vec<RunningApp>,
    pub clock: String,
    pub uptime_secs: u64,
    pub sessiond_rss_mb: u64,
}

impl SessionSnapshot {
    pub fn new(focused_workspace: u8, running_apps: Vec<RunningApp>) -> Self {
        Self {
            focused_workspace,
            running_apps,
            clock: now_clock(),
            uptime_secs: 0,
            sessiond_rss_mb: memory::current_rss_mb().unwrap_or(0),
        }
    }

    /// Títulos dos apps em execução (compatibilidade com a UI simples do panel).
    pub fn titles(&self) -> Vec<String> {
        self.running_apps.iter().map(|a| a.title.clone()).collect()
    }
}

/// Hora local atual no formato `HH:MM`.
pub fn now_clock() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// Data/hora local atual no formato `HH:MM:SS DD/MM/AAAA`.
pub fn now_full() -> String {
    chrono::Local::now().format("%H:%M:%S %d/%m/%Y").to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuwiConfigV1 {
    pub version: u16,
    pub theme: ThemeConfig,
    pub panel: PanelConfig,
    pub memory_budget: memory::MemoryBudget,
    pub apps: Vec<DesktopEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub accent: String,
    /// Estilo do papel de parede: "bliss" (céu/colina), "solid" ou "gradient".
    pub wallpaper: String,
    /// Nome exibido no cabeçalho do Menu Iniciar.
    #[serde(default = "default_user_name")]
    pub user_name: String,
}

fn default_user_name() -> String {
    std::env::var("USER").unwrap_or_else(|_| "Usuário".into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelConfig {
    pub height: f32,
    pub show_clock: bool,
    pub start_label: String,
}

impl Default for LuwiConfigV1 {
    fn default() -> Self {
        Self {
            version: 1,
            theme: ThemeConfig {
                accent: "#245edb".into(),
                wallpaper: "bliss".into(),
                user_name: default_user_name(),
            },
            panel: PanelConfig {
                height: 40.0,
                show_clock: true,
                start_label: "Iniciar".into(),
            },
            memory_budget: memory::MemoryBudget::default(),
            apps: default_apps(),
        }
    }
}

/// Loja de configuração: carrega `luwide.toml`, cria com padrões se não existir,
/// salva alterações e detecta mudanças externas (hot-reload).
pub struct ConfigStore {
    path: PathBuf,
    last_modified: Option<SystemTime>,
    cached: LuwiConfigV1,
}

impl ConfigStore {
    pub fn load_or_create() -> Result<Self> {
        let path = config_path()?;
        Self::load_or_create_at(path)
    }

    pub fn load_or_create_at(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let default = LuwiConfigV1::default();
            fs::write(&path, toml::to_string_pretty(&default)?)?;
        }
        let cached = load_config(&path)?;
        let modified = fs::metadata(&path).and_then(|m| m.modified()).ok();
        Ok(Self { path, last_modified: modified, cached })
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn current(&self) -> &LuwiConfigV1 {
        &self.cached
    }

    pub fn current_mut(&mut self) -> &mut LuwiConfigV1 {
        &mut self.cached
    }

    /// Persiste a configuração atual em disco.
    pub fn save(&mut self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, toml::to_string_pretty(&self.cached)?)
            .with_context(|| format!("Falha ao salvar config em {}", self.path.display()))?;
        self.last_modified = fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        Ok(())
    }

    pub fn reload_if_changed(&mut self) -> Result<bool> {
        let modified = fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        if modified.is_some() && modified != self.last_modified {
            self.cached = load_config(&self.path)?;
            self.last_modified = modified;
            return Ok(true);
        }
        Ok(false)
    }
}

fn load_config(path: &PathBuf) -> Result<LuwiConfigV1> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Falha ao ler config em {}", path.display()))?;
    let parsed: LuwiConfigV1 = toml::from_str(&raw)
        .with_context(|| format!("Falha ao parsear TOML em {}", path.display()))?;
    if parsed.version != 1 {
        anyhow::bail!("Versão de configuração não suportada: {}", parsed.version);
    }
    Ok(parsed)
}

pub fn config_path() -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("LUWIDE_CONFIG") {
        if !custom.is_empty() {
            return Ok(PathBuf::from(custom));
        }
    }
    let dirs = ProjectDirs::from("org", "Luwi", "LuwiDE")
        .ok_or_else(|| anyhow::anyhow!("Não foi possível localizar diretório de configuração"))?;
    Ok(dirs.config_dir().join("luwide.toml"))
}

/// Diretório home do usuário (sem depender de crate externa além de `directories`).
pub fn home_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Aplicativos padrão do LuwiDE. Os quatro primeiros são internos (builtin);
/// os demais são lançadores externos comuns em qualquer Linux.
pub fn default_apps() -> Vec<DesktopEntry> {
    vec![
        DesktopEntry { id: "explorer".into(), title: "Explorer".into(), exec: "builtin:explorer".into(), icon: "🗂".into(), builtin: true },
        DesktopEntry { id: "terminal".into(), title: "Terminal".into(), exec: "builtin:terminal".into(), icon: "💻".into(), builtin: true },
        DesktopEntry { id: "notepad".into(), title: "Bloco de Notas".into(), exec: "builtin:notepad".into(), icon: "📝".into(), builtin: true },
        DesktopEntry { id: "calculator".into(), title: "Calculadora".into(), exec: "builtin:calculator".into(), icon: "🧮".into(), builtin: true },
        DesktopEntry { id: "browser".into(), title: "Navegador".into(), exec: "firefox".into(), icon: "🌐".into(), builtin: false },
        DesktopEntry { id: "settings".into(), title: "Configurações".into(), exec: "builtin:settings".into(), icon: "⚙".into(), builtin: true },
        DesktopEntry { id: "about".into(), title: "Sobre o LuwiDE".into(), exec: "builtin:about".into(), icon: "ℹ".into(), builtin: true },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_roundtrip_toml() {
        let cfg = LuwiConfigV1::default();
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: LuwiConfigV1 = toml::from_str(&text).unwrap();
        assert_eq!(back.version, 1);
        assert_eq!(back.panel.start_label, "Iniciar");
        assert_eq!(back.apps.len(), cfg.apps.len());
        assert_eq!(back.memory_budget.total_mb(), 200);
    }

    #[test]
    fn config_store_persiste_e_recarrega() {
        let dir = std::env::temp_dir().join(format!("luwide-test-{}", std::process::id()));
        let path = dir.join("luwide.toml");
        let mut store = ConfigStore::load_or_create_at(path.clone()).unwrap();
        assert!(path.exists());
        store.current_mut().panel.start_label = "Start".into();
        store.save().unwrap();
        let store2 = ConfigStore::load_or_create_at(path).unwrap();
        assert_eq!(store2.current().panel.start_label, "Start");
        let _ = std::fs::remove_dir_all(dir);
    }
}
