//! Estado e lógica dos aplicativos internos do LuwiDE
//! (Explorer, Terminal, Bloco de Notas). A renderização egui fica em
//! `shell.rs`; aqui ficam as partes testáveis sem interface gráfica.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/* ================================ Explorer ================================ */

#[derive(Debug, Clone)]
pub struct FileItem {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct ExplorerState {
    pub path: PathBuf,
    pub error: Option<String>,
}

impl ExplorerState {
    pub fn new(path: PathBuf) -> Self {
        Self { path, error: None }
    }

    /// Lista o diretório atual: pastas primeiro, ordem alfabética.
    pub fn list(&self) -> Vec<FileItem> {
        list_dir(&self.path)
    }

    pub fn navigate(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.path = path;
            self.error = None;
        } else {
            self.error = Some(format!("'{}' não é uma pasta", path.display()));
        }
    }

    pub fn up(&mut self) {
        if let Some(parent) = self.path.parent().map(|p| p.to_path_buf()) {
            self.navigate(parent);
        }
    }
}

pub fn list_dir(path: &Path) -> Vec<FileItem> {
    let Ok(read) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut items: Vec<FileItem> = read
        .flatten()
        .map(|e| {
            let meta = e.metadata().ok();
            FileItem {
                name: e.file_name().to_string_lossy().to_string(),
                path: e.path(),
                is_dir: meta.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                size: meta.map(|m| m.len()).unwrap_or(0),
            }
        })
        .collect();
    items.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    items
}

/// Formata um tamanho em bytes de forma legível.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/* ================================ Terminal ================================ */

#[derive(Debug, Clone)]
pub struct TerminalState {
    pub cwd: PathBuf,
    pub lines: Vec<String>,
    pub input: String,
}

pub enum TermAction {
    None,
    Clear,
    Exit,
}

impl TerminalState {
    pub fn new(cwd: PathBuf) -> Self {
        Self {
            cwd,
            lines: vec![
                "LuwiDE Terminal — digite 'help' para ajuda.".into(),
                String::new(),
            ],
            input: String::new(),
        }
    }

    /// Executa o comando digitado (via `sh -c`, com comandos internos
    /// para `cd`, `clear`, `exit`, `pwd` e `help`).
    pub fn run_input(&mut self) -> TermAction {
        let input = self.input.trim().to_string();
        self.input.clear();
        self.lines.push(format!("{} $ {input}", prompt_dir(&self.cwd)));
        if input.is_empty() {
            return TermAction::None;
        }
        let mut parts = input.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();
        match cmd {
            "clear" => {
                self.lines.clear();
                return TermAction::Clear;
            }
            "exit" => return TermAction::Exit,
            "pwd" => self.lines.push(self.cwd.display().to_string()),
            "help" => {
                for l in [
                    "Comandos internos: cd <pasta>, pwd, clear, exit, help",
                    "Qualquer outro comando é executado via /bin/sh -c.",
                    "Exemplos: ls -la, echo olá, cat arquivo.txt, df -h",
                ] {
                    self.lines.push(l.into());
                }
            }
            "cd" => {
                let target = if rest.is_empty() {
                    luwide_common::home_dir()
                } else if rest == "~" {
                    luwide_common::home_dir()
                } else {
                    let p = PathBuf::from(rest);
                    if p.is_absolute() { p } else { self.cwd.join(p) }
                };
                match target.canonicalize() {
                    Ok(p) if p.is_dir() => self.cwd = p,
                    _ => self.lines.push(format!("cd: pasta não encontrada: {rest}")),
                }
            }
            _ => {
                let (out, code) = exec_shell(&self.cwd, &input);
                for line in out.lines() {
                    self.lines.push(line.to_string());
                }
                if code != 0 {
                    self.lines.push(format!("[código de saída: {code}]"));
                }
            }
        }
        // Mantém o histórico limitado (memória baixa é um objetivo do projeto).
        if self.lines.len() > 500 {
            let keep = self.lines.split_off(self.lines.len() - 500);
            self.lines = keep;
        }
        TermAction::None
    }
}

fn prompt_dir(cwd: &Path) -> String {
    let home = luwide_common::home_dir();
    match cwd.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => cwd.display().to_string(),
    }
}

/// Executa um comando de shell capturando stdout+stderr combinados.
pub fn exec_shell(cwd: &Path, command: &str) -> (String, i32) {
    match Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
    {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).to_string();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            if text.len() > 64_000 {
                text.truncate(64_000);
                text.push_str("\n[…saída truncada…]");
            }
            (text, out.status.code().unwrap_or(-1))
        }
        Err(e) => (format!("erro ao executar: {e}"), -1),
    }
}

/* ============================== Bloco de Notas ============================== */

#[derive(Debug, Clone)]
pub struct NotepadState {
    pub path_text: String,
    pub text: String,
    pub status: String,
    pub dirty: bool,
}

impl NotepadState {
    pub fn new() -> Self {
        Self { path_text: String::new(), text: String::new(), status: "Novo documento".into(), dirty: false }
    }

    pub fn open(&mut self) {
        let path = PathBuf::from(self.path_text.trim());
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                self.text = content;
                self.dirty = false;
                self.status = format!("Aberto: {}", path.display());
            }
            Err(e) => self.status = format!("Erro ao abrir: {e}"),
        }
    }

    pub fn save(&mut self) {
        let path = PathBuf::from(self.path_text.trim());
        if path.as_os_str().is_empty() {
            self.status = "Informe o caminho do arquivo para salvar.".into();
            return;
        }
        match std::fs::write(&path, &self.text) {
            Ok(()) => {
                self.dirty = false;
                self.status = format!("Salvo: {} ({} bytes)", path.display(), self.text.len());
            }
            Err(e) => self.status = format!("Erro ao salvar: {e}"),
        }
    }
}

impl Default for NotepadState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_echo_e_cd() {
        let mut t = TerminalState::new(std::env::temp_dir());
        t.input = "echo luwide-funciona".into();
        t.run_input();
        assert!(t.lines.iter().any(|l| l.contains("luwide-funciona")));
        t.input = "cd /".into();
        t.run_input();
        assert_eq!(t.cwd, PathBuf::from("/"));
        t.input = "cd pasta-que-nao-existe-xyz".into();
        t.run_input();
        assert!(t.lines.iter().any(|l| l.contains("não encontrada")));
    }

    #[test]
    fn explorer_lista_e_tamanhos() {
        let dir = std::env::temp_dir().join(format!("luwide-explorer-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pasta")).unwrap();
        std::fs::write(dir.join("arquivo.txt"), b"12345").unwrap();
        let items = list_dir(&dir);
        assert_eq!(items.len(), 2);
        assert!(items[0].is_dir, "pastas vêm primeiro");
        assert_eq!(human_size(5), "5 B");
        assert_eq!(human_size(2048), "2.0 KB");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn notepad_salva_e_abre() {
        let path = std::env::temp_dir().join(format!("luwide-notepad-{}.txt", std::process::id()));
        let mut n = NotepadState::new();
        n.path_text = path.display().to_string();
        n.text = "Olá, LuwiDE!".into();
        n.save();
        assert!(!n.dirty);
        let mut n2 = NotepadState::new();
        n2.path_text = path.display().to_string();
        n2.open();
        assert_eq!(n2.text, "Olá, LuwiDE!");
        let _ = std::fs::remove_file(path);
    }
}
