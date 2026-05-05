use anyhow::{Context, Result};
use calloop::EventLoop;
use std::{env, fs, path::PathBuf, time::Duration};
use tracing::{info, warn};
use wayland_server::{backend::ClientData, Display, ListeningSocket};

#[derive(Debug)]
struct LuwiClientData;
impl ClientData for LuwiClientData {}

fn main() -> Result<()> {
    init_tracing();

    let runtime_dir = resolve_runtime_dir()?;
    let socket_name = "wayland-luwi";
    let socket_path = runtime_dir.join(socket_name);
    if socket_path.exists() {
        fs::remove_file(&socket_path)
            .with_context(|| format!("Falha ao remover socket antigo: {}", socket_path.display()))?;
    }

    let mut display: Display<LuwiState> = Display::new().context("Falha ao criar display Wayland")?;
    let mut event_loop: EventLoop<LuwiState> = EventLoop::try_new().context("Falha ao criar event loop")?;

    let listening_socket = ListeningSocket::bind(&socket_path)
        .with_context(|| format!("Falha ao criar socket Wayland em {}", socket_path.display()))?;

    info!("LuwiDE compositor iniciado");
    info!("WAYLAND_DISPLAY={}", socket_name);
    info!("Socket: {}", socket_path.display());

    let mut state = LuwiState::new(socket_name.to_owned(), socket_path);
    loop {
        while let Some(stream) = listening_socket.accept().transpose().context("Erro ao aceitar cliente Wayland")? {
            let client = display
                .handle()
                .insert_client(stream, std::sync::Arc::new(LuwiClientData))
                .context("Falha ao registrar cliente Wayland")?;
            info!(?client, "Cliente Wayland conectado");
        }

        event_loop
            .dispatch(Duration::from_millis(16), &mut state)
            .context("Falha no dispatch do compositor")?;

        display.flush_clients().context("Falha ao flush em clientes Wayland")?;
    }
}

#[derive(Debug)]
struct LuwiState {
    socket_name: String,
    socket_path: PathBuf,
}

impl LuwiState {
    fn new(socket_name: String, socket_path: PathBuf) -> Self {
        Self { socket_name, socket_path }
    }
}

impl Drop for LuwiState {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.socket_path) {
            warn!(?error, path = %self.socket_path.display(), "Falha ao remover socket Wayland");
            return;
        }
        info!(display = %self.socket_name, "Socket Wayland removido");
    }
}

fn resolve_runtime_dir() -> Result<PathBuf> {
    if let Ok(value) = env::var("XDG_RUNTIME_DIR") {
        let path = PathBuf::from(value);
        if path.exists() {
            return Ok(path);
        }
    }

    let fallback = env::temp_dir().join("luwide-runtime");
    fs::create_dir_all(&fallback)
        .with_context(|| format!("Não foi possível criar runtime dir fallback: {}", fallback.display()))?;
    Ok(fallback)
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
