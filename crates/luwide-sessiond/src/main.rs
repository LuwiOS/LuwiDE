//! `luwide-sessiond` — daemon de sessão do LuwiDE.
//!
//! Responsabilidades reais:
//! - serve o protocolo IPC (`luwide_common::ipc`) num socket Unix;
//! - lança e rastreia aplicativos externos (spawn + espera de saída);
//! - mantém o estado da sessão (workspace focado, apps em execução, relógio);
//! - monitora o próprio RSS contra o orçamento de memória da config.
//!
//! Uso: `luwide-sessiond` (fica em primeiro plano, como um daemon de sessão).

use anyhow::{Context, Result};
use luwide_common::{
    ipc::{socket_path, IpcRequest, IpcResponse},
    memory::current_rss_mb,
    ConfigStore, RunningApp, SessionSnapshot,
};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

struct SessionState {
    focused_workspace: u8,
    running: HashMap<u32, RunningApp>,
    started: Instant,
}

impl SessionState {
    fn snapshot(&self) -> SessionSnapshot {
        let mut apps: Vec<RunningApp> = self.running.values().cloned().collect();
        apps.sort_by_key(|a| a.started_unix);
        let mut snap = SessionSnapshot::new(self.focused_workspace, apps);
        snap.uptime_secs = self.started.elapsed().as_secs();
        snap
    }
}

struct Daemon {
    state: Mutex<SessionState>,
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Divide "firefox --new-window" em (programa, args). Aspas simples não são
/// suportadas de propósito: comandos de `.desktop` não as usam na prática.
pub fn split_exec(exec: &str) -> Option<(String, Vec<String>)> {
    let mut parts = exec.split_whitespace();
    let program = parts.next()?.to_string();
    Some((program, parts.map(|s| s.to_string()).collect()))
}

impl Daemon {
    fn handle(self: &Arc<Self>, req: IpcRequest, shutdown: &AtomicBool) -> IpcResponse {
        match req {
            IpcRequest::Ping => IpcResponse::Pong,
            IpcRequest::GetSnapshot => {
                IpcResponse::Snapshot { snapshot: self.state.lock().unwrap().snapshot() }
            }
            IpcRequest::SetWorkspace { workspace } => {
                self.state.lock().unwrap().focused_workspace = workspace.clamp(1, 9);
                IpcResponse::Ok
            }
            IpcRequest::RegisterApp { pid, title, exec } => {
                let mut state = self.state.lock().unwrap();
                state.running.insert(
                    pid,
                    RunningApp { pid, title, exec, workspace: state.focused_workspace, started_unix: now_unix() },
                );
                IpcResponse::Ok
            }
            IpcRequest::LaunchApp { exec, title } => match self.launch(&exec, &title) {
                Ok(app) => IpcResponse::Launched { app },
                Err(e) => IpcResponse::Error { message: e.to_string() },
            },
            IpcRequest::Shutdown => {
                shutdown.store(true, Ordering::SeqCst);
                IpcResponse::Ok
            }
        }
    }

    fn launch(self: &Arc<Self>, exec: &str, title: &str) -> Result<RunningApp> {
        let (program, args) = split_exec(exec)
            .ok_or_else(|| anyhow::anyhow!("comando vazio"))?;
        let mut child = Command::new(&program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("falha ao executar '{program}' (instalado?)"))?;
        let pid = child.id();
        let app = RunningApp {
            pid,
            title: title.to_string(),
            exec: exec.to_string(),
            workspace: self.state.lock().unwrap().focused_workspace,
            started_unix: now_unix(),
        };
        self.state.lock().unwrap().running.insert(pid, app.clone());
        // Vigia a saída do processo para removê-lo do estado da sessão.
        let daemon = Arc::clone(self);
        std::thread::spawn(move || {
            let _ = child.wait();
            daemon.state.lock().unwrap().running.remove(&pid);
            daemon.children.lock().unwrap().remove(&pid);
            log(&format!("app encerrado: pid={pid}"));
        });
        // Guarda o Child já "movido" para a thread? Não: o wait consome.
        // (children fica apenas para apps registrados externamente — vazio aqui.)
        log(&format!("app lançado: '{exec}' pid={pid}"));
        Ok(app)
    }
}

fn log(msg: &str) {
    eprintln!("[luwide-sessiond {}] {msg}", luwide_common::now_full());
}

fn handle_client(daemon: Arc<Daemon>, stream: UnixStream, shutdown: Arc<AtomicBool>) {
    let Ok(mut writer) = stream.try_clone() else { return };
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<IpcRequest>(trimmed) {
            Ok(req) => daemon.handle(req, &shutdown),
            Err(e) => IpcResponse::Error { message: format!("requisição inválida: {e}") },
        };
        let Ok(text) = serde_json::to_string(&response) else { break };
        if writer.write_all(text.as_bytes()).is_err()
            || writer.write_all(b"\n").is_err()
            || writer.flush().is_err()
        {
            break;
        }
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
    }
}

fn main() -> Result<()> {
    let config = ConfigStore::load_or_create()?;
    let budget_mb = config.current().memory_budget.sessiond_mb;

    let path = socket_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Se já existe um sessiond vivo, não subimos outro.
    if path.exists() {
        if luwide_common::ipc::sessiond_alive() {
            log("já existe um luwide-sessiond em execução; saindo");
            return Ok(());
        }
        let _ = std::fs::remove_file(&path); // socket órfão
    }
    let listener = UnixListener::bind(&path)
        .with_context(|| format!("falha ao escutar em {}", path.display()))?;
    listener.set_nonblocking(true)?;
    log(&format!("sessiond online — socket: {}", path.display()));
    log(&format!(
        "orçamento de memória: total={}MB sessiond={}MB",
        config.current().memory_budget.total_mb(),
        budget_mb
    ));

    let daemon = Arc::new(Daemon {
        state: Mutex::new(SessionState { focused_workspace: 1, running: HashMap::new(), started: Instant::now() }),
    });
    let shutdown = Arc::new(AtomicBool::new(false));

    // Monitor de memória (avisa se estourar o orçamento).
    {
        let shutdown = Arc::clone(&shutdown);
        std::thread::spawn(move || {
            while !shutdown.load(Ordering::SeqCst) {
                if let Some(rss) = current_rss_mb() {
                    if rss > budget_mb as u64 {
                        log(&format!("⚠ RSS {rss}MB acima do orçamento de {budget_mb}MB"));
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(15));
            }
        });
    }

    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let daemon = Arc::clone(&daemon);
                let shutdown = Arc::clone(&shutdown);
                std::thread::spawn(move || handle_client(daemon, stream, shutdown));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                log(&format!("erro ao aceitar conexão: {e}"));
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    log("encerrando a pedido de um cliente (shutdown)");
    let _ = std::fs::remove_file(&path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_exec_ok() {
        let (prog, args) = split_exec("firefox --new-window").unwrap();
        assert_eq!(prog, "firefox");
        assert_eq!(args, vec!["--new-window".to_string()]);
        assert!(split_exec("   ").is_none());
        let (prog, args) = split_exec("/bin/true").unwrap();
        assert_eq!(prog, "/bin/true");
        assert!(args.is_empty());
    }
}
