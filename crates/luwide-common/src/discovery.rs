//! Descoberta de aplicativos instalados a partir de arquivos `.desktop`
//! (padrão freedesktop), para o Menu Iniciar mostrar programas reais.

use crate::DesktopEntry;
use std::{collections::HashSet, path::PathBuf};

/// Diretórios padrão de `applications/` (sistema + usuário).
pub fn applications_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
    ];
    if let Some(data) = directories::BaseDirs::new().map(|b| b.data_dir().to_path_buf()) {
        dirs.push(data.join("applications"));
    }
    dirs
}

/// Faz o parse de um arquivo `.desktop` (apenas a seção `[Desktop Entry]`).
/// Retorna `None` para entradas ocultas/sem comando.
pub fn parse_desktop_file(content: &str, id: &str) -> Option<DesktopEntry> {
    let mut name = None::<String>;
    let mut exec = None::<String>;
    let mut icon = String::from("📦");
    let mut in_entry = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        match key.trim() {
            "Name" if name.is_none() => name = Some(value.to_string()),
            "Exec" if exec.is_none() => exec = Some(value.to_string()),
            "Icon" => icon = value.to_string(),
            "NoDisplay" | "Hidden" if value.eq_ignore_ascii_case("true") => return None,
            "Type" if value != "Application" => return None,
            _ => {}
        }
    }
    let exec = clean_exec(&exec?);
    if exec.is_empty() {
        return None;
    }
    Some(DesktopEntry {
        id: id.to_string(),
        title: name.filter(|n| !n.is_empty())?,
        exec,
        icon,
        builtin: false,
    })
}

/// Remove códigos de campo freedesktop (`%f`, `%F`, `%u`, `%U`, `%i`, ...)
/// e aspas, deixando um comando executável simples.
pub fn clean_exec(exec: &str) -> String {
    let mut out = Vec::new();
    for token in exec.split_whitespace() {
        if token == "--" {
            continue;
        }
        if token.starts_with('%') && token.len() <= 2 {
            continue;
        }
        out.push(token.trim_matches('"'));
    }
    out.join(" ")
}

/// Varre os diretórios freedesktop e devolve os apps encontrados,
/// ordenados por título e sem duplicar comandos.
pub fn discover_desktop_apps() -> Vec<DesktopEntry> {
    let mut apps = Vec::new();
    let mut seen = HashSet::new();
    for dir in applications_dirs() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "desktop") != Some(true) {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else { continue };
            let id = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "app".into());
            if let Some(app) = parse_desktop_file(&content, &id) {
                if seen.insert(app.exec.clone()) {
                    apps.push(app);
                }
            }
        }
    }
    apps.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    apps
}

/// Junta apps da config + descobertos, sem duplicar por `exec`.
pub fn merged_apps(config_apps: &[DesktopEntry]) -> Vec<DesktopEntry> {
    let mut apps: Vec<DesktopEntry> = config_apps.to_vec();
    let mut seen: HashSet<String> = apps.iter().map(|a| a.exec.clone()).collect();
    for app in discover_desktop_apps() {
        if seen.insert(app.exec.clone()) {
            apps.push(app);
        }
    }
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_desktop_ok() {
        let content = "[Desktop Entry]\nType=Application\nName=Firefox\nExec=firefox %u\nIcon=firefox\n";
        let app = parse_desktop_file(content, "firefox").unwrap();
        assert_eq!(app.title, "Firefox");
        assert_eq!(app.exec, "firefox");
        assert!(!app.builtin);
    }

    #[test]
    fn parse_desktop_oculto_e_sem_exec() {
        let hidden = "[Desktop Entry]\nName=X\nExec=x\nNoDisplay=true\n";
        assert!(parse_desktop_file(hidden, "x").is_none());
        let noexec = "[Desktop Entry]\nName=X\n";
        assert!(parse_desktop_file(noexec, "x").is_none());
    }

    #[test]
    fn clean_exec_remove_codigos() {
        assert_eq!(clean_exec("gimp %U"), "gimp");
        assert_eq!(clean_exec("code --new-window %F"), "code --new-window");
    }
}
