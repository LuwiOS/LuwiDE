pub mod memory;
pub mod theme;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, time::SystemTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopEntry {
    pub id: String,
    pub title: String,
    pub exec: String,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub focused_workspace: u8,
    pub running_apps: Vec<String>,
    pub clock: String,
}

impl SessionSnapshot {
    pub fn new(focused_workspace: u8, running_apps: Vec<String>) -> Self {
        Self {
            focused_workspace,
            running_apps,
            clock: chrono::Local::now().format("%H:%M").to_string(),
        }
    }
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
    pub wallpaper: String,
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
                accent: "#2563be".into(),
                wallpaper: "assets/wallpapers/bliss.svg".into(),
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

pub struct ConfigStore {
    path: PathBuf,
    last_modified: Option<SystemTime>,
    cached: LuwiConfigV1,
}

impl ConfigStore {
    pub fn load_or_create() -> Result<Self> {
        let path = config_path()?;
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

    pub fn current(&self) -> &LuwiConfigV1 {
        &self.cached
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

fn config_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("org", "Luwi", "LuwiDE")
        .ok_or_else(|| anyhow::anyhow!("Não foi possível localizar diretório de configuração"))?;
    Ok(dirs.config_dir().join("luwide.toml"))
}

pub fn default_apps() -> Vec<DesktopEntry> {
    vec![
        DesktopEntry { id: "explorer".into(), title: "Explorer".into(), exec: "luwi-explorer".into(), icon: "folder.svg".into() },
        DesktopEntry { id: "terminal".into(), title: "Terminal".into(), exec: "alacritty".into(), icon: "terminal.svg".into() },
        DesktopEntry { id: "browser".into(), title: "Browser".into(), exec: "firefox".into(), icon: "browser.svg".into() },
    ]
}
