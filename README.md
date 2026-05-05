# LuwiDE

LuwiDE é um desktop environment leve para Linux escrito em Rust, com visual nostálgico inspirado no Windows XP.

## Estado atual

A arquitetura foi separada em componentes reais de DE:

- **`luwide-compositor`**: sessão Wayland real (socket e loop de clientes).
- **`luwide-shell`**: superfície de desktop (wallpaper/área principal).
- **`luwide-panel`**: barra inferior estilo XP.
- **`luwide-launcher`**: menu iniciar (launcher).
- **`luwide-sessiond`**: daemon de sessão (estado compartilhado).
- **`luwide-common`**: tipos compartilhados + configuração versionada.

## Meta de memória (~200MB) com orçamento por módulo

Agora o arquivo `luwide.toml` inclui `memory_budget` versionado:

- `compositor_mb = 70`
- `shell_mb = 60`
- `panel_mb = 20`
- `launcher_mb = 25`
- `sessiond_mb = 25`

**Total alvo: 200MB** (`MemoryBudget::total_mb`).

- `sessiond` monitora `VmRSS` via `/proc/self/status` e alerta quando ultrapassa seu budget.
- `panel`, `launcher` e `shell` exibem seus budgets atuais na UI.

## Tema XP nostálgico (mais completo e leve)

- Paleta XP centralizada em `luwide-common::theme::XpTheme`.
- Barra com destaque superior, botão Iniciar arredondado e área de tray.
- Launcher com aparência de janela clássica azul/branco.
- Desktop com céu/grama para lembrar o estilo clássico.
- Ícones SVG adicionais para tray: rede, volume e bateria.

## Executar (desenvolvimento)

```bash
cargo run -p luwide-compositor
cargo run -p luwide-sessiond
cargo run -p luwide-shell
cargo run -p luwide-panel
cargo run -p luwide-launcher
```
