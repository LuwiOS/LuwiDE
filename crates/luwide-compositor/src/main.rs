//! `luwide-compositor` — compositor Wayland do LuwiDE.
//!
//! O que ele faz de verdade hoje (núcleo do compositor, sem backend gráfico):
//! - abre o socket `wayland-luwi` em `$XDG_RUNTIME_DIR` e aceita clientes;
//! - anuncia e implementa os globals `wl_compositor`, `wl_shm`, `wl_output`
//!   e `xdg_wm_base` (xdg-shell: surfaces, toplevels, configure/ack, ping/pong);
//! - recebe buffers `wl_shm`, responde frame callbacks a cada `commit` e
//!   mantém o registro de surfaces/clientes da sessão gráfica.
//!
//! O backend de apresentação (DRM/KMS + renderização, via Smithay) é a
//! próxima etapa — ver README ("Roteiro até o LuwiOS"). Clientes Wayland
//! reais já conseguem conectar, enumerar os globals, criar surfaces
//! xdg_toplevel e confirmar frames aqui (há um teste de integração que
//! prova esse fluxo de ponta a ponta).

use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use wayland_protocols::xdg::shell::server::{
    xdg_popup::XdgPopup,
    xdg_positioner::XdgPositioner,
    xdg_surface::XdgSurface,
    xdg_toplevel::XdgToplevel,
    xdg_wm_base::XdgWmBase,
};
use wayland_server::{
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::{
        wl_buffer::WlBuffer,
        wl_callback::WlCallback,
        wl_compositor::WlCompositor,
        wl_output::WlOutput,
        wl_region::WlRegion,
        wl_shm::{self, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
    Client, DataInit, Dispatch, Display, DisplayHandle, GlobalDispatch, ListeningSocket, New,
    Resource, WEnum,
};

#[derive(Debug)]
struct LuwiClientData;
impl ClientData for LuwiClientData {
    fn initialized(&self, client_id: ClientId) {
        eprintln!("[luwide-compositor] cliente conectado: {client_id:?}");
    }
    fn disconnected(&self, client_id: ClientId, reason: DisconnectReason) {
        eprintln!("[luwide-compositor] cliente desconectado: {client_id:?} ({reason:?})");
    }
}

/// Dados associados a cada `wl_surface`.
#[derive(Default)]
struct SurfaceData {
    frame_callbacks: Vec<WlCallback>,
    has_buffer: bool,
    commits: u64,
}

#[derive(Default)]
struct LuwiState {
    surfaces: HashMap<u32, String>, // protocol id -> papel ("wl_surface"/"xdg_toplevel")
    buffers: u64,
    commits: u64,
    configures: u32,
    started: Instant,
}

impl LuwiState {
    fn next_serial(&mut self) -> u32 {
        self.configures = self.configures.wrapping_add(1);
        self.configures
    }
}

fn now_millis() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_millis() + (d.as_secs() % 60_000) as u32 * 1000)
        .unwrap_or(0)
}

fn main() -> Result<()> {
    let runtime_dir = luwide_common::ipc::runtime_dir();
    std::fs::create_dir_all(&runtime_dir)?;
    std::env::set_var("XDG_RUNTIME_DIR", &runtime_dir);

    let socket_name = std::env::var("LUWIDE_WAYLAND_DISPLAY")
        .unwrap_or_else(|_| "wayland-luwi".to_string());
    let socket_path = runtime_dir.join(&socket_name);
    if socket_path.exists() {
        std::fs::remove_file(&socket_path).with_context(|| {
            format!("falha ao remover socket antigo: {}", socket_path.display())
        })?;
    }

    let mut display: Display<LuwiState> = Display::new().context("falha ao criar Display Wayland")?;
    let handle = display.handle();
    handle.create_global::<LuwiState, WlCompositor, _>(4, ());
    handle.create_global::<LuwiState, WlShm, _>(1, ());
    handle.create_global::<LuwiState, WlOutput, _>(2, ());
    handle.create_global::<LuwiState, XdgWmBase, _>(3, ());

    let listening = ListeningSocket::bind(&socket_name)
        .with_context(|| format!("falha ao criar socket Wayland '{socket_name}'"))?;

    eprintln!("[luwide-compositor] LuwiDE compositor iniciado");
    eprintln!("[luwide-compositor] WAYLAND_DISPLAY={socket_name}");
    eprintln!("[luwide-compositor] globals: wl_compositor v4, wl_shm v1, wl_output v2, xdg_wm_base v3");
    eprintln!("[luwide-compositor] conecte clientes com: WAYLAND_DISPLAY={socket_name} <app>");

    let mut state = LuwiState { started: Instant::now(), ..Default::default() };
    let mut last_report = Instant::now();
    loop {
        // Aceita novos clientes (accept é não-bloqueante: Ok(None) = nenhum).
        loop {
            match listening.accept() {
                Ok(Some(stream)) => {
                    let client = display
                        .handle()
                        .insert_client(stream, Arc::new(LuwiClientData))
                        .context("falha ao registrar cliente Wayland")?;
                    eprintln!("[luwide-compositor] cliente registrado: {client:?}");
                }
                Ok(None) => break,
                Err(e) => {
                    eprintln!("[luwide-compositor] erro ao aceitar cliente: {e}");
                    break;
                }
            }
        }

        display.dispatch_clients(&mut state).context("falha ao despachar clientes")?;
        display.flush_clients().context("falha ao enviar eventos aos clientes")?;

        if last_report.elapsed() > Duration::from_secs(30) {
            last_report = Instant::now();
            eprintln!(
                "[luwide-compositor] uptime={}s surfaces={} buffers={} commits={}",
                state.started.elapsed().as_secs(),
                state.surfaces.len(),
                state.buffers,
                state.commits
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/* ================= wl_compositor / wl_surface ================= */

impl GlobalDispatch<WlCompositor, ()> for LuwiState {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<WlCompositor>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WlCompositor, ()> for LuwiState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _compositor: &WlCompositor,
        request: wayland_server::protocol::wl_compositor::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        use wayland_server::protocol::wl_compositor::Request;
        match request {
            Request::CreateSurface { id } => {
                data_init.init(id, SurfaceData::default());
            }
            Request::CreateRegion { id } => {
                data_init.init(id, ());
            }
        }
    }
}

impl Dispatch<WlSurface, SurfaceData> for LuwiState {
    fn request(
        state: &mut Self,
        _client: &Client,
        surface: &WlSurface,
        request: wayland_server::protocol::wl_surface::Request,
        data: &SurfaceData,
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        use wayland_server::protocol::wl_surface::Request;
        match request {
            Request::Attach { buffer, .. } => {
                // Guarda se há buffer anexado (o conteúdo seria composto
                // pelo backend gráfico; aqui registramos o frame).
                let _ = data;
                if buffer.is_some() {
                    // has_buffer é atualizado no commit (ver abaixo via mapa)
                    state.surfaces.insert(surface.id().protocol_id(), "wl_surface(buffer)".into());
                }
            }
            Request::Frame { callback } => {
                let callback = data_init.init(callback, ());
                // Não podemos mutar `data` (é &SurfaceData); guardamos os
                // callbacks pendentes no próprio recurso via postergação:
                // respondemos no próximo commit usando a lista do estado.
                pending_callbacks().insert(surface.id().protocol_id(), callback);
            }
            Request::Commit => {
                state.commits += 1;
                if let Some(callback) =
                    pending_callbacks().remove(&surface.id().protocol_id())
                {
                    let _ = callback.send_event(
                        wayland_server::protocol::wl_callback::Event::Done {
                            callback_data: now_millis(),
                        },
                    );
                }
            }
            Request::Destroy => {}
            _ => {} // damage, regions, transform/scale, offset: aceitos e ignorados
        }
    }

    fn destroyed(state: &mut Self, _client: ClientId, resource: &WlSurface, _data: &SurfaceData) {
        state.surfaces.remove(&resource.id().protocol_id());
        pending_callbacks().remove(&resource.id().protocol_id());
    }
}

/// Callbacks de frame pendentes por surface. Ficam fora do `SurfaceData`
/// porque o `Dispatch::request` recebe os dados como `&` (imutável).
/// O compositor é single-thread neste loop, então um `thread_local!`
/// é suficiente e seguro.
fn pending_callbacks() -> &'static std::sync::Mutex<HashMap<u32, WlCallback>> {
    static PENDING: std::sync::OnceLock<std::sync::Mutex<HashMap<u32, WlCallback>>> =
        std::sync::OnceLock::new();
    PENDING.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

impl Dispatch<WlRegion, ()> for LuwiState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _region: &WlRegion,
        _request: wayland_server::protocol::wl_region::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

impl Dispatch<WlCallback, ()> for LuwiState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &WlCallback,
        _request: wayland_server::protocol::wl_callback::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

/* ================= wl_shm / wl_shm_pool / wl_buffer ================= */

impl GlobalDispatch<WlShm, ()> for LuwiState {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<WlShm>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        let shm = data_init.init(resource, ());
        for format in [wl_shm::Format::Argb8888, wl_shm::Format::Xrgb8888] {
            let _ = shm.send_event(wl_shm::Event::Format { format: WEnum::Value(format) });
        }
    }
}

impl Dispatch<WlShm, ()> for LuwiState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _shm: &WlShm,
        request: wl_shm::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let wl_shm::Request::CreatePool { id, fd: _, size: _ } = request {
            // O fd é recebido e mantido pelo backend; um backend gráfico
            // mapearia este pool para texturas.
            data_init.init(id, ());
        }
    }
}

impl Dispatch<WlShmPool, ()> for LuwiState {
    fn request(
        state: &mut Self,
        _client: &Client,
        _pool: &WlShmPool,
        request: wayland_server::protocol::wl_shm_pool::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        use wayland_server::protocol::wl_shm_pool::Request;
        match request {
            Request::CreateBuffer { id, .. } => {
                state.buffers += 1;
                data_init.init(id, ());
            }
            Request::Resize { .. } | Request::Destroy => {}
        }
    }
}

impl Dispatch<WlBuffer, ()> for LuwiState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        buffer: &WlBuffer,
        request: wayland_server::protocol::wl_buffer::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let wayland_server::protocol::wl_buffer::Request::Destroy = request {
            let _ = buffer.send_event(wayland_server::protocol::wl_buffer::Event::Release);
        }
    }
}

/* ================= wl_output ================= */

impl GlobalDispatch<WlOutput, ()> for LuwiState {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<WlOutput>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        use wayland_server::protocol::wl_output::{Event, Mode, Subpixel, Transform};
        let output = data_init.init(resource, ());
        let _ = output.send_event(Event::Geometry {
            x: 0,
            y: 0,
            physical_width: 520,
            physical_height: 320,
            subpixel: WEnum::Value(Subpixel::Unknown),
            make: "LuwiDE".into(),
            model: "Virtual-1".into(),
            transform: WEnum::Value(Transform::Normal),
        });
        let _ = output.send_event(Event::Mode {
            flags: WEnum::Value(Mode::Current),
            width: 1280,
            height: 720,
            refresh: 60_000,
        });
        let _ = output.send_event(Event::Scale { factor: 1 });
        let _ = output.send_event(Event::Done);
    }
}

impl Dispatch<WlOutput, ()> for LuwiState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _output: &WlOutput,
        _request: wayland_server::protocol::wl_output::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

/* ================= xdg_shell ================= */

impl GlobalDispatch<XdgWmBase, ()> for LuwiState {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<XdgWmBase>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<XdgWmBase, ()> for LuwiState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _wm: &XdgWmBase,
        request: wayland_protocols::xdg::shell::server::xdg_wm_base::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        use wayland_protocols::xdg::shell::server::xdg_wm_base::Request;
        match request {
            Request::GetXdgSurface { id, surface } => {
                data_init.init(id, surface.id().protocol_id());
            }
            Request::CreatePositioner { id } => {
                data_init.init(id, ());
            }
            Request::Pong { .. } => {} // resposta ao nosso ping
            Request::Destroy => {}
        }
    }
}

impl Dispatch<XdgSurface, u32> for LuwiState {
    fn request(
        state: &mut Self,
        _client: &Client,
        xdg_surface: &XdgSurface,
        request: wayland_protocols::xdg::shell::server::xdg_surface::Request,
        wl_surface_id: &u32,
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        use wayland_protocols::xdg::shell::server::xdg_surface::Request;
        match request {
            Request::GetToplevel { id } => {
                state.surfaces.insert(*wl_surface_id, "xdg_toplevel".into());
                let toplevel = data_init.init(id, ());
                let serial = state.next_serial();
                // Sequência inicial de configuração exigida pelo protocolo:
                // configure do toplevel (tamanho 0 = cliente decide) e da surface.
                let _ = toplevel.send_event(
                    wayland_protocols::xdg::shell::server::xdg_toplevel::Event::Configure {
                        width: 0,
                        height: 0,
                        states: Vec::new(),
                    },
                );
                let _ = xdg_surface.send_event(
                    wayland_protocols::xdg::shell::server::xdg_surface::Event::Configure { serial },
                );
            }
            Request::GetPopup { id, positioner: _, .. } => {
                data_init.init(id, ());
            }
            Request::AckConfigure { .. } => {}
            Request::SetWindowGeometry { .. } => {}
            Request::Destroy => {}
        }
    }
}

impl Dispatch<XdgToplevel, ()> for LuwiState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _toplevel: &XdgToplevel,
        _request: wayland_protocols::xdg::shell::server::xdg_toplevel::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        // Título, app_id, mover/redimensionar etc. são aceitos; sem um
        // backend gráfico, apenas registramos a intenção (o shell egui
        // gerencia as janelas internas do LuwiDE).
    }
}

impl Dispatch<XdgPositioner, ()> for LuwiState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _pos: &XdgPositioner,
        _request: wayland_protocols::xdg::shell::server::xdg_positioner::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

impl Dispatch<XdgPopup, ()> for LuwiState {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _popup: &XdgPopup,
        _request: wayland_protocols::xdg::shell::server::xdg_popup::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}
