# LuwiDE

**LuwiDE** é um ambiente de trabalho (Desktop Environment) leve para Linux, escrito em **Rust**, com visual nostálgico inspirado no **Windows XP** (tema Luna: barra azul, botão Iniciar verde, wallpaper "Bliss").

Ele está sendo desenvolvido para ser, no futuro, a shell padrão do **LuwiOS** — um sistema operacional Linux que será escrito em Rust (exceto o kernel).

> Estado: **funcional (v0.2.0)**. A shell abre, gerencia janelas, o Menu Iniciar lista os programas reais da máquina, o sessiond lança e rastreia processos, e o compositor aceita clientes Wayland de verdade. Tudo coberto por testes (unitários + integração) na CI.

## Componentes

| Crate | Binário | O que faz |
|---|---|---|
| `luwide-compositor` | `luwide-compositor` | Compositor Wayland: socket `wayland-luwi`, globals `wl_compositor` v4, `wl_shm`, `wl_output`, `xdg_wm_base` (xdg-shell com configure/ack e ping/pong), buffers shm, frame callbacks a cada commit e registro de surfaces. |
| `luwide-shell` | `luwide-shell` | A área de trabalho: wallpaper Bliss, ícones, **gerenciador de janelas** (arrastar, redimensionar, minimizar, maximizar, foco/z-order), barra de tarefas, Menu Iniciar com busca, bandeja com relógio e status do sessiond. |
| `luwide-sessiond` | `luwide-sessiond` | Daemon de sessão: serve o protocolo IPC (JSON por linhas em socket Unix), lança e rastreia aplicativos externos (spawn + vigia de saída), mantém workspace/apps/relógio e monitora o próprio RSS contra o orçamento. |
| `luwide-panel` | `luwide-panel` | Barra de tarefas autônoma estilo XP (para usar com outras shells/compositores). Botão Iniciar abre o launcher; mostra apps do sessiond e o relógio. |
| `luwide-launcher` | `luwide-launcher` | Menu Iniciar autônomo: busca e lança programas reais via sessiond. |
| `luwide-common` | — (biblioteca) | Configuração versionada (`luwide.toml` com hot-reload), tema XP centralizado, orçamento de memória, protocolo IPC e descoberta de apps `.desktop` (freedesktop). |

### Aplicativos internos da shell (funcionam de verdade)

- **Explorer** — navega pelo sistema de arquivos real (pastas primeiro, tamanhos, dois cliques para entrar/abrir com `xdg-open`).
- **Terminal** — executa comandos reais via `/bin/sh -c`, com `cd`/`pwd`/`clear`/`exit`/`help` internos e histórico de saída.
- **Bloco de Notas** — abre, edita e salva arquivos de texto de verdade.
- **Calculadora** — aritmética com encadeamento de operações.
- **Configurações** — usuário, rótulo do botão Iniciar, relógio, papel de parede (Bliss/gradiente/sólida) e cor de destaque; salva em `luwide.toml` (com hot-reload nos demais componentes).
- **Monitor do Sistema** — RSS do processo, janelas abertas, apps externos rastreados pelo sessiond e o orçamento de memória por módulo.
- **Sobre o LuwiDE** — informações da sessão.

Aplicativos externos do Menu Iniciar vêm dos arquivos `.desktop` da máquina (`/usr/share/applications`, `~/.local/share/applications`) mesclados aos da configuração, e são lançados/rastreados pelo **sessiond** (ou direto, em modo degradado, se o daemon estiver fora do ar — a UI nunca trava por isso).

## Como executar

Pré-requisitos: Rust estável (`rustup`) e, no Linux, os pacotes de desenvolvimento do Wayland/X11/GL (ex.: `libwayland-dev libxkbcommon-dev libx11-dev libxcb1-dev libgl1-mesa-dev` no Debian/Ubuntu).

Sessão completa (compositor + sessiond + shell):

```bash
cargo build --release --workspace
./scripts/luwide-session.sh
```

Ou componente a componente (desenvolvimento):

```bash
cargo run -p luwide-compositor   # terminal 1: WAYLAND_DISPLAY=wayland-luwi
cargo run -p luwide-sessiond     # terminal 2: daemon de sessão (IPC)
cargo run -p luwide-shell        # terminal 3: a área de trabalho
cargo run -p luwide-panel        # opcional: barra autônoma
cargo run -p luwide-launcher     # opcional: Menu Iniciar autônomo
```

Clientes Wayland podem se conectar ao compositor com:

```bash
WAYLAND_DISPLAY=wayland-luwi seu-app-wayland
```

## Testes

```bash
cargo test --workspace
```

Inclui testes unitários (gerenciador de janelas, calculadora, terminal, explorer, bloco de notas, config, descoberta `.desktop`, protocolo IPC, orçamento) e **testes de integração que sobem os binários reais**:

- `luwide-sessiond`: lança `/bin/sleep`, confere o rastreamento no snapshot, a remoção após a saída e o tratamento de comandos inexistentes — tudo via IPC.
- `luwide-compositor`: um cliente Wayland real (`wayland-client`) enumera os globals, cria surface + buffer `wl_shm`, abre um `xdg_toplevel`, confirma o `configure` e faz commits sem erro de protocolo.

A CI (`.github/workflows/ci.yml`) roda `cargo test --workspace` e o build de release a cada push.

## Configuração

Arquivo: `~/.config/Luwi/LuwiDE/luwide.toml` (ou `$LUWIDE_CONFIG`), criado automaticamente na primeira execução, com hot-reload:

```toml
version = 1

[theme]
accent = "#245edb"
wallpaper = "bliss"        # bliss | gradient | solid
user_name = "voce"

[panel]
height = 40.0
show_clock = true
start_label = "Iniciar"

[memory_budget]            # meta total: 200 MB
compositor_mb = 70
shell_mb = 60
panel_mb = 20
launcher_mb = 25
sessiond_mb = 25

[[apps]]                   # apps fixos do desktop/Menu Iniciar
id = "terminal"
title = "Terminal"
exec = "builtin:terminal"
icon = "💻"
builtin = true
```

## Protocolo IPC (shell/panel/launcher ↔ sessiond)

JSON por linhas sobre `$XDG_RUNTIME_DIR/luwide/session.sock` (fallback: `/tmp/luwide-$USER/luwide/session.sock`; sobrescrevível com `$LUWIDE_SESSION_SOCKET`). Mensagens: `ping`, `get_snapshot`, `launch_app {exec,title}`, `register_app`, `set_workspace`, `shutdown` → `pong`, `snapshot`, `launched`, `ok`, `error`. Definições em `crates/luwide-common/src/ipc.rs`.

## Roteiro até o LuwiOS

1. **Backend gráfico do compositor** (Smithay: DRM/KMS, GBM/EGL, apresentação real das surfaces, entrada teclado/mouse, `wl_seat`) — transformando o núcleo de protocolo atual em compositor de sessão completa. A interface interna já isola essa parte: o registro de surfaces/commits é o ponto de integração.
2. **Sessão gráfica instalável**: arquivo de sessão para GDM/SDDM (`assets/luwide.desktop` já incluído), `systemd --user` units e instalador.
3. **Apps próprios** para o LuwiOS reaproveitando `luwide-common` (mesma config, tema e IPC): gerenciador de arquivos standalone, terminal próprio e central de configurações.
4. **Tema XP completo**: ícones SVG próprios (já há um conjunto inicial em `assets/icons`), sons e telas de login/logoff.

## Licença

MIT — veja os metadados do workspace. Contribuições são bem-vindas.
