//! Teste de integração: sobe o binário real do compositor e conecta um
//! cliente Wayland de verdade (wayland-client), enumerando os globals,
//! criando uma surface + buffer shm, um xdg_toplevel e confirmando frames.

use std::{
    io::Write,
    os::unix::io::AsFd,
    process::{Child, Command},
    time::{Duration, Instant},
};
use wayland_client::{
    delegate_noop,
    globals::{registry_queue_init, GlobalListContents},
    protocol::{wl_buffer, wl_compositor, wl_output, wl_registry, wl_shm, wl_shm_pool, wl_surface},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

struct ClientState {
    configured: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ClientState {
    fn event(
        _state: &mut Self,
        _registry: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for ClientState {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.configured = true;
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for ClientState {
    fn event(
        _state: &mut Self,
        wm: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm.pong(serial);
        }
    }
}

delegate_noop!(ClientState: ignore wl_compositor::WlCompositor);
delegate_noop!(ClientState: ignore wl_surface::WlSurface);
delegate_noop!(ClientState: ignore wl_shm::WlShm);
delegate_noop!(ClientState: ignore wl_shm_pool::WlShmPool);
delegate_noop!(ClientState: ignore wl_buffer::WlBuffer);
delegate_noop!(ClientState: ignore wl_output::WlOutput);
delegate_noop!(ClientState: ignore xdg_toplevel::XdgToplevel);

struct Guard(Child, std::path::PathBuf);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

#[test]
fn compositor_aceita_cliente_wayland_real() {
    let runtime =
        std::env::temp_dir().join(format!("luwide-compositor-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&runtime);
    std::fs::create_dir_all(&runtime).unwrap();

    let child = Command::new(env!("CARGO_BIN_EXE_luwide-compositor"))
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("LUWIDE_WAYLAND_DISPLAY", "wayland-luwitest")
        .env("LUWIDE_CONFIG", runtime.join("luwide.toml"))
        .spawn()
        .expect("spawn compositor");
    let _guard = Guard(child, runtime.clone());

    let socket = runtime.join("wayland-luwitest");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !socket.exists() {
        assert!(Instant::now() < deadline, "socket do compositor não apareceu");
        std::thread::sleep(Duration::from_millis(50));
    }

    std::env::set_var("XDG_RUNTIME_DIR", &runtime);
    std::env::set_var("WAYLAND_DISPLAY", "wayland-luwitest");

    let conn = Connection::connect_to_env().expect("conectar ao compositor");
    let (globals, mut queue) = registry_queue_init::<ClientState>(&conn).expect("registry init");
    let qh = queue.handle();
    let mut state = ClientState { configured: false };

    let names: Vec<String> = globals
        .contents()
        .with_list(|list| list.iter().map(|g| g.interface.clone()).collect());
    for expected in ["wl_compositor", "wl_shm", "wl_output", "xdg_wm_base"] {
        assert!(
            names.iter().any(|n| n == expected),
            "global {expected} ausente; encontrados: {names:?}"
        );
    }

    let compositor: wl_compositor::WlCompositor = globals.bind(&qh, 1..=4, ()).unwrap();
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).unwrap();
    let wm: xdg_wm_base::XdgWmBase = globals.bind(&qh, 1..=3, ()).unwrap();
    let _output: wl_output::WlOutput = globals.bind(&qh, 1..=2, ()).unwrap();

    // Surface + buffer shm real (arquivo temporário como pool de memória).
    let surface = compositor.create_surface(&qh, ());
    let file = tempfile_shm(64 * 64 * 4);
    let pool: wl_shm_pool::WlShmPool = shm.create_pool(file.as_fd(), 64 * 64 * 4, &qh, ());
    let buffer = pool.create_buffer(0, 64, 64, 64 * 4, wl_shm::Format::Xrgb8888, &qh, ());
    surface.attach(Some(&buffer), 0, 0);

    // xdg surface + toplevel, como um aplicativo gráfico real faria.
    let xdg_surface_obj = wm.get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg_surface_obj.get_toplevel(&qh, ());
    toplevel.set_title("Teste LuwiDE".into());
    toplevel.set_app_id("org.luwi.teste".into());
    surface.commit();
    queue.roundtrip(&mut state).expect("roundtrip xdg");

    // Mais frames: o servidor deve responder sem erro de protocolo.
    surface.damage(0, 0, 64, 64);
    surface.commit();
    queue.roundtrip(&mut state).expect("roundtrip commit 2");
    assert!(state.configured, "xdg_surface.configure não foi recebido/confirmado");

    // A conexão continua saudável após todo o fluxo.
    surface.commit();
    queue.roundtrip(&mut state).expect("roundtrip final");
}

fn tempfile_shm(size: usize) -> std::fs::File {
    let path = std::env::temp_dir().join(format!("luwide-shm-{}-{}.bin", std::process::id(), size));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(&vec![0x5Au8; size]).unwrap();
    f.flush().unwrap();
    let _ = std::fs::remove_file(&path);
    f
}
