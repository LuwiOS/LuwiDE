//! Teste de integração: sobe o binário real do sessiond num diretório
//! temporário e conversa com ele pelo protocolo IPC.

use luwide_common::ipc::{request_at, IpcRequest, IpcResponse};
use std::{path::PathBuf, process::{Child, Command}, time::{Duration, Instant}};

struct Guard(Child, PathBuf);
impl Drop for Guard {
    fn drop(&mut self) {
        let sock = self.1.join("luwide").join("session.sock");
        let _ = request_at(&sock, &IpcRequest::Shutdown);
        let _ = self.0.kill();
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

fn wait_for<F: Fn() -> bool>(what: &str, f: F) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timeout esperando: {what}");
}

#[test]
fn sessiond_ipc_fluxo_completo() {
    let runtime = std::env::temp_dir().join(format!("luwide-sessiond-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&runtime);
    std::fs::create_dir_all(&runtime).unwrap();
    let sock = runtime.join("luwide").join("session.sock");

    let child = Command::new(env!("CARGO_BIN_EXE_luwide-sessiond"))
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("LUWIDE_CONFIG", runtime.join("luwide.toml"))
        .spawn()
        .expect("spawn sessiond");
    let _guard = Guard(child, runtime.clone());

    wait_for("socket do sessiond", || sock.exists());
    wait_for("ping", || matches!(request_at(&sock, &IpcRequest::Ping), Ok(IpcResponse::Pong)));

    // Lança um processo real e confere o rastreamento.
    let launched = request_at(&sock, &IpcRequest::LaunchApp {
        exec: "/bin/sleep 3".into(),
        title: "SleepTest".into(),
    })
    .expect("launch");
    let pid = match launched {
        IpcResponse::Launched { app } => {
            assert_eq!(app.title, "SleepTest");
            app.pid
        }
        other => panic!("resposta inesperada: {other:?}"),
    };

    wait_for("snapshot com o app", || match request_at(&sock, &IpcRequest::GetSnapshot) {
        Ok(IpcResponse::Snapshot { snapshot }) => {
            snapshot.running_apps.iter().any(|a| a.pid == pid)
        }
        _ => false,
    });

    // Após o sleep terminar, o sessiond deve removê-lo sozinho.
    wait_for("app removido após saída", || match request_at(&sock, &IpcRequest::GetSnapshot) {
        Ok(IpcResponse::Snapshot { snapshot }) => {
            snapshot.running_apps.iter().all(|a| a.pid != pid)
        }
        _ => false,
    });

    // Comando inexistente vira erro tratado (não derruba o daemon).
    match request_at(&sock, &IpcRequest::LaunchApp {
        exec: "/bin/luwide-nao-existe-xyz".into(),
        title: "X".into(),
    }) {
        Ok(IpcResponse::Error { .. }) => {}
        other => panic!("esperava Error, veio: {other:?}"),
    }
    assert!(matches!(request_at(&sock, &IpcRequest::Ping), Ok(IpcResponse::Pong)));

    // Workspace muda e aparece no snapshot.
    assert!(matches!(
        request_at(&sock, &IpcRequest::SetWorkspace { workspace: 3 }),
        Ok(IpcResponse::Ok)
    ));
    match request_at(&sock, &IpcRequest::GetSnapshot) {
        Ok(IpcResponse::Snapshot { snapshot }) => assert_eq!(snapshot.focused_workspace, 3),
        other => panic!("snapshot inesperado: {other:?}"),
    }
}
