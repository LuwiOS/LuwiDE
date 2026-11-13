#!/usr/bin/env bash
# ============================================================================
#  LuwiDE — instalador do Desktop Environment
# ============================================================================
#  Instala na sua máquina:
#    • os 5 binários Rust: luwide-compositor, luwide-shell, luwide-panel,
#      luwide-launcher e luwide-sessiond
#    • o lançador de sessão `luwide-session` (sobe compositor + sessiond + shell)
#    • ícones e wallpaper em <prefix>/share/luwide/
#    • a entrada de sessão do LuwiDE para gerenciadores de login
#      (GDM/SDDM/LightDM): <prefix>/share/{wayland-sessions,xsessions}/luwide.desktop
#
#  Uso:
#    ./install.sh                 # instalação do sistema em /usr/local (usa sudo)
#    ./install.sh --user          # só para o seu usuário, em ~/.local (sem sudo)
#    ./install.sh --prefix /opt/luwide
#    ./install.sh --no-deps       # não instala dependências do sistema
#    ./install.sh --no-build      # usa binários já compilados em target/release
#    ./install.sh --dry-run       # apenas mostra o que faria
#    ./install.sh --uninstall     # remove tudo (mantém sua configuração)
#    ./install.sh --uninstall --user
#
#  Requer: bash, git/curl (só se faltar o Rust) e conexão com a internet.
#  Distros com dependências automáticas: Debian/Ubuntu, Fedora, Arch e openSUSE.
# ============================================================================
set -euo pipefail

# ------------------------------------------------------------------ opções --
MODE="system"            # system | user
PREFIX=""
INSTALL_DEPS=1
BUILD=1
UNINSTALL=0
DRY_RUN=0

usage() {
    sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
    case "$1" in
        --user)      MODE="user" ;;
        --system)    MODE="system" ;;
        --prefix)    PREFIX="${2:?--prefix requer um caminho}"; shift ;;
        --no-deps)   INSTALL_DEPS=0 ;;
        --no-build)  BUILD=0 ;;
        --uninstall) UNINSTALL=1 ;;
        --dry-run)   DRY_RUN=1 ;;
        -h|--help)   usage; exit 0 ;;
        *) echo "Opção desconhecida: $1 (use --help)" >&2; exit 2 ;;
    esac
    shift
done

# --------------------------------------------------------------- caminhos --
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -z "$PREFIX" ]; then
    if [ "$MODE" = "user" ]; then
        PREFIX="$HOME/.local"
    else
        PREFIX="/usr/local"
    fi
fi
BINDIR="$PREFIX/bin"
DATADIR="$PREFIX/share/luwide"
ICONDIR="$PREFIX/share/icons/hicolor/scalable/apps"
WAYLAND_SESSIONS_DIR="$PREFIX/share/wayland-sessions"
XSESSIONS_DIR="$PREFIX/share/xsessions"
APPLICATIONS_DIR="$PREFIX/share/applications"

BINARIES="luwide-compositor luwide-shell luwide-panel luwide-launcher luwide-sessiond"

# sudo apenas quando necessário (instalação do sistema fora do $HOME)
SUDO=""
if [ "$MODE" = "system" ] && [ "$(id -u)" -ne 0 ]; then
    if command -v sudo >/dev/null 2>&1; then
        SUDO="sudo"
    else
        echo "⚠  'sudo' não encontrado; tentando sem ele (pode falhar por permissão)." >&2
    fi
fi

# ---------------------------------------------------------------- helpers --
C_GREEN='\033[1;32m'; C_BLUE='\033[1;34m'; C_YELLOW='\033[1;33m'; C_OFF='\033[0m'
log()  { printf "${C_BLUE}==>${C_OFF} %s\n" "$*"; }
ok()   { printf "${C_GREEN} ✓${C_OFF} %s\n" "$*"; }
warn() { printf "${C_YELLOW} ⚠ %s${C_OFF}\n" "$*" >&2; }

# Executa (ou apenas mostra, com --dry-run) um comando, elevando com sudo
# quando o destino é do sistema. "$@" é executado via env, sem shell.
run() {
    if [ "$DRY_RUN" -eq 1 ]; then
        printf '   [dry-run] %s %s\n' "$SUDO" "$*"
    elif [ -n "$SUDO" ]; then
        $SUDO "$@"
    else
        "$@"
    fi
}

# run_user: nunca usa sudo (build e passos do próprio usuário).
run_user() {
    if [ "$DRY_RUN" -eq 1 ]; then
        printf '   [dry-run] %s\n' "$*"
    else
        "$@"
    fi
}

die() { echo "Erro: $*" >&2; exit 1; }

# ----------------------------------------------------------- desinstalação --
if [ "$UNINSTALL" -eq 1 ]; then
    log "Removendo o LuwiDE de $PREFIX …"
    for bin in $BINARIES luwide-session; do
        run rm -f "$BINDIR/$bin"
    done
    run rm -f "$WAYLAND_SESSIONS_DIR/luwide.desktop" \
              "$XSESSIONS_DIR/luwide.desktop" \
              "$APPLICATIONS_DIR/luwide.desktop" \
              "$ICONDIR/luwide.svg"
    run rm -rf "$DATADIR"
    ok "LuwiDE removido."
    echo "Sua configuração foi mantida em: ~/.config/Luwi/LuwiDE/luwide.toml"
    echo "Para removê-la também:  rm -rf ~/.config/Luwi"
    exit 0
fi

# ------------------------------------------------------------- dependências --
install_system_deps() {
    [ "$INSTALL_DEPS" -eq 1 ] || { log "Pulando dependências do sistema (--no-deps)"; return; }
    log "Instalando dependências de compilação e execução…"

    if command -v apt-get >/dev/null 2>&1; then
        # Debian / Ubuntu / derivados
        run apt-get update
        run apt-get install -y --no-install-recommends \
            build-essential pkg-config curl git \
            libwayland-dev wayland-protocols \
            libxkbcommon-dev libx11-dev \
            libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
            libgl1-mesa-dev libfontconfig1-dev
    elif command -v dnf >/dev/null 2>&1; then
        # Fedora / RHEL
        run dnf install -y \
            gcc gcc-c++ make pkgconf-pkg-config curl git \
            wayland-devel wayland-protocols-devel \
            libxkbcommon-devel libX11-devel \
            libxcb-devel mesa-libGL-devel fontconfig-devel
    elif command -v pacman >/dev/null 2>&1; then
        # Arch / Manjaro
        run pacman -Sy --noconfirm --needed \
            base-devel pkgconf curl git \
            wayland wayland-protocols \
            libxkbcommon libx11 libxcb mesa fontconfig
    elif command -v zypper >/dev/null 2>&1; then
        # openSUSE
        run zypper --non-interactive install \
            gcc gcc-c++ make pkg-config curl git \
            wayland-devel wayland-protocols-devel \
            libxkbcommon-devel libX11-devel \
            libxcb-devel Mesa-libGL-devel fontconfig-devel
    else
        warn "Gerenciador de pacotes não reconhecido (apt/dnf/pacman/zypper)."
        warn "Instale manualmente: compilador C, pkg-config, wayland (+protocols),"
        warn "libxkbcommon, X11/xcb e Mesa/GL — depois rode com --no-deps."
        return 0
    fi
    ok "Dependências instaladas"
}

# ------------------------------------------------------------------- Rust --
ensure_rust() {
    if command -v cargo >/dev/null 2>&1; then
        ok "Rust encontrado: $(rustc --version 2>/dev/null || echo 'versão desconhecida')"
        return
    fi
    # cargo instalado pelo rustup mas ainda fora do PATH desta sessão?
    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck disable=SC1091
        . "$HOME/.cargo/env"
    fi
    if command -v cargo >/dev/null 2>&1; then
        ok "Rust encontrado via rustup: $(rustc --version)"
        return
    fi
    log "Rust não encontrado — instalando via rustup (oficial)…"
    if [ "$DRY_RUN" -eq 1 ]; then
        printf '   [dry-run] curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y\n'
        return
    fi
    if command -v curl >/dev/null 2>&1; then
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- https://sh.rustup.rs | sh -s -- -y
    else
        die "instale 'curl' ou 'wget' (ou o Rust de https://rustup.rs) e tente de novo"
    fi
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
    command -v cargo >/dev/null 2>&1 || die "rustup instalado, mas 'cargo' não entrou no PATH"
    ok "Rust instalado: $(rustc --version)"
}

# ------------------------------------------------------------------ build --
build_binaries() {
    [ "$BUILD" -eq 1 ] || { log "Usando binários existentes em target/release (--no-build)"; return; }
    log "Compilando o LuwiDE em modo release (pode levar alguns minutos)…"
    run_user cargo build --release --workspace --manifest-path "$SCRIPT_DIR/Cargo.toml"
    ok "Compilação concluída"
}

# ------------------------------------------------------------------ install --
install_files() {
    log "Instalando arquivos em $PREFIX …"

    # Binários compilados
    run install -d "$BINDIR"
    for bin in $BINARIES; do
        src="$SCRIPT_DIR/target/release/$bin"
        if [ "$DRY_RUN" -eq 0 ] && [ ! -x "$src" ]; then
            die "binário não encontrado: $src (compile antes, sem --no-build)"
        fi
        run install -m 0755 "$src" "$BINDIR/$bin"
    done
    ok "Binários instalados em $BINDIR"

    # Lançador de sessão (script que sobe compositor + sessiond + shell)
    run install -m 0755 "$SCRIPT_DIR/scripts/luwide-session.sh" "$BINDIR/luwide-session"
    ok "Sessão: $BINDIR/luwide-session"

    # Ícones + wallpaper
    run install -d "$DATADIR/icons" "$DATADIR/wallpapers"
    for f in "$SCRIPT_DIR"/assets/icons/*.svg; do
        run install -m 0644 "$f" "$DATADIR/icons/"
    done
    for f in "$SCRIPT_DIR"/assets/wallpapers/*; do
        [ -f "$f" ] && run install -m 0644 "$f" "$DATADIR/wallpapers/"
    done
    run install -d "$ICONDIR"
    run install -m 0644 "$SCRIPT_DIR/assets/icons/start.svg" "$ICONDIR/luwide.svg"
    ok "Ícones e wallpaper em $DATADIR"

    # Entradas .desktop: app + sessão para gerenciadores de login,
    # já apontando para o caminho instalado (independe do PATH).
    run install -d "$APPLICATIONS_DIR" "$WAYLAND_SESSIONS_DIR" "$XSESSIONS_DIR"
    tmp_desktop="$(mktemp)"
    sed -e "s|^Exec=.*|Exec=$BINDIR/luwide-session|" \
        -e "s|^TryExec=.*|TryExec=$BINDIR/luwide-shell|" \
        "$SCRIPT_DIR/assets/luwide.desktop" > "$tmp_desktop"
    run install -m 0644 "$tmp_desktop" "$APPLICATIONS_DIR/luwide.desktop"
    run install -m 0644 "$tmp_desktop" "$WAYLAND_SESSIONS_DIR/luwide.desktop"
    run install -m 0644 "$tmp_desktop" "$XSESSIONS_DIR/luwide.desktop"
    rm -f "$tmp_desktop"
    ok "Entrada de sessão instalada (Wayland/X11) para GDM/SDDM/LightDM"
}

# -------------------------------------------------------------------- main --
echo "============================================================"
echo " LuwiDE — Desktop Environment (Rust, visual XP nostálgico)"
echo "============================================================"
echo " Modo:       $MODE"
echo " Prefixo:    $PREFIX"
echo " Binários:   $BINDIR"
[ "$DRY_RUN" -eq 1 ] && echo " ** MODO DRY-RUN: nada será alterado **"
echo

install_system_deps
# Com --no-build os binários já estão prontos: o Rust não é necessário.
if [ "$BUILD" -eq 1 ]; then
    ensure_rust
fi
build_binaries
install_files

echo
ok "LuwiDE instalado com sucesso! 🎉"
echo
echo "Para iniciar agora (dentro da sua sessão gráfica atual):"
echo "    luwide-session"
if [ "$MODE" = "user" ]; then
    case ":$PATH:" in
        *":$HOME/.local/bin:"*) ;;
        *) echo "    (adicione ~/.local/bin ao PATH se o comando não for achado)" ;;
    esac
fi
echo
echo "Ou saia da sessão e escolha 'LuwiDE' na tela de login"
echo "(funciona melhor em instalações de sistema: ./install.sh)."
echo
echo "Componentes individuais, se quiser testar um a um:"
echo "    luwide-compositor   # compositor Wayland (socket wayland-luwi)"
echo "    luwide-sessiond     # daemon de sessão (IPC)"
echo "    luwide-shell        # área de trabalho"
echo "    luwide-panel        # barra de tarefas autônoma"
echo "    luwide-launcher     # Menu Iniciar autônomo"
echo
echo "Configuração (criada na 1ª execução): ~/.config/Luwi/LuwiDE/luwide.toml"
echo "Desinstalar:  ./install.sh --uninstall$([ "$MODE" = user ] && echo ' --user')"
