//! Protocolo IPC do LuwiDE: mensagens JSON (uma por linha) sobre um
//! socket Unix servido pelo `luwide-sessiond`.
//!
//! Shell, panel e launcher são clientes deste protocolo; se o sessiond
//! não estiver no ar, eles operam em modo degradado (sem rastrear apps
//! externos), nunca travando a interface.

use crate::{RunningApp, SessionSnapshot};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

/// Mensagens cliente → sessiond.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcRequest {
    Ping,
    GetSnapshot,
    /// Pede ao sessiond para lançar (e rastrear) um aplicativo externo.
    LaunchApp { exec: String, title: String },
    /// Registra um app lançado por fora (ex.: pelo próprio shell).
    RegisterApp { pid: u32, title: String, exec: String },
    SetWorkspace { workspace: u8 },
    Shutdown,
}

/// Respostas sessiond → cliente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcResponse {
    Pong,
    Ok,
    Snapshot { snapshot: SessionSnapshot },
    Launched { app: RunningApp },
    Error { message: String },
}

/// Caminho do socket IPC: `$XDG_RUNTIME_DIR/luwide/session.sock`,
/// com fallback para `/tmp/luwide-$USER/session.sock`.
pub fn socket_path() -> PathBuf {
    if let Ok(custom) = std::env::var("LUWIDE_SESSION_SOCKET") {
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
    }
    runtime_dir().join("luwide").join("session.sock")
}

/// `XDG_RUNTIME_DIR` válido, ou fallback em `/tmp`.
pub fn runtime_dir() -> PathBuf {
    if let Ok(value) = std::env::var("XDG_RUNTIME_DIR") {
        let path = PathBuf::from(&value);
        if !value.is_empty() && path.is_absolute() {
            return path;
        }
    }
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    std::env::temp_dir().join(format!("luwide-{user}"))
}

/// Envia uma requisição ao sessiond e lê a resposta (timeout curto:
/// a UI nunca pode travar esperando o daemon).
pub fn request(req: &IpcRequest) -> Result<IpcResponse> {
    request_at(&socket_path(), req)
}

pub fn request_at(path: &PathBuf, req: &IpcRequest) -> Result<IpcResponse> {
    let mut stream = UnixStream::connect(path)
        .with_context(|| format!("sessiond indisponível em {}", path.display()))?;
    stream.set_read_timeout(Some(Duration::from_millis(900)))?;
    stream.set_write_timeout(Some(Duration::from_millis(900)))?;
    let line = serde_json::to_string(req)?;
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    let mut reader = BufReader::new(stream);
    let mut reply = String::new();
    reader.read_line(&mut reply)?;
    if reply.trim().is_empty() {
        anyhow::bail!("sessiond fechou a conexão sem responder");
    }
    Ok(serde_json::from_str(reply.trim())?)
}

/// `true` se o sessiond responde a um Ping.
pub fn sessiond_alive() -> bool {
    matches!(request(&IpcRequest::Ping), Ok(IpcResponse::Pong))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_roundtrip_serde() {
        let req = IpcRequest::LaunchApp { exec: "firefox".into(), title: "Navegador".into() };
        let text = serde_json::to_string(&req).unwrap();
        assert!(text.contains("\"launch_app\""));
        let back: IpcRequest = serde_json::from_str(&text).unwrap();
        assert_eq!(back, req);

        let resp = IpcResponse::Snapshot {
            snapshot: SessionSnapshot::new(2, vec![]),
        };
        let text = serde_json::to_string(&resp).unwrap();
        let back: IpcResponse = serde_json::from_str(&text).unwrap();
        assert!(matches!(back, IpcResponse::Snapshot { .. }));
    }

    #[test]
    fn socket_path_respeita_env() {
        std::env::set_var("LUWIDE_SESSION_SOCKET", "/tmp/luwide-test.sock");
        assert_eq!(socket_path(), PathBuf::from("/tmp/luwide-test.sock"));
        std::env::remove_var("LUWIDE_SESSION_SOCKET");
    }
}
