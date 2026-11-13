#!/bin/sh
# Sessão completa do LuwiDE (para uso em um tty/gerenciador de login
# ou dentro de outra sessão para desenvolvimento).
#
# Ordem de subida:
#   1. luwide-compositor  -> socket Wayland "wayland-luwi"
#   2. luwide-sessiond    -> daemon de sessão (IPC)
#   3. luwide-shell       -> área de trabalho (desktop, janelas, barra)
#
# O panel e o launcher são processos separados/legados da arquitetura:
# a shell integrada já inclui barra de tarefas e Menu Iniciar. Eles
# continuam funcionais e podem ser usados com outros compositores.

set -eu

BIN_DIR="$(cd "$(dirname "$0")/.." && pwd)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/luwide-$(id -un)}"
mkdir -p "$XDG_RUNTIME_DIR"

# Binários: prefere os do PATH; em desenvolvimento, usa target/release.
find_bin() {
    if command -v "$1" >/dev/null 2>&1; then
        command -v "$1"
    elif [ -x "$BIN_DIR/target/release/$1" ]; then
        echo "$BIN_DIR/target/release/$1"
    elif [ -x "$BIN_DIR/target/debug/$1" ]; then
        echo "$BIN_DIR/target/debug/$1"
    else
        echo "$1"
    fi
}

COMPOSITOR="$(find_bin luwide-compositor)"
SESSIOND="$(find_bin luwide-sessiond)"
SHELL_BIN="$(find_bin luwide-shell)"

echo "[luwide-session] subindo compositor…"
"$COMPOSITOR" &
COMP_PID=$!

echo "[luwide-session] subindo sessiond…"
"$SESSIOND" &
SESS_PID=$!

# Dá tempo dos sockets aparecerem
sleep 1

echo "[luwide-session] subindo shell (área de trabalho)…"
"$SHELL_BIN"
EXIT_CODE=$?

echo "[luwide-session] shell encerrou; finalizando a sessão"
kill "$SESS_PID" "$COMP_PID" 2>/dev/null || true
exit "$EXIT_CODE"
