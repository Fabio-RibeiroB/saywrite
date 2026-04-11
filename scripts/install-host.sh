#!/usr/bin/env bash
set -euo pipefail

BINDIR="${HOME}/.local/bin"
DBUS_DIR="${HOME}/.local/share/dbus-1/services"
SYSTEMD_DIR="${HOME}/.config/systemd/user"
SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SELF_DIR}/.." && pwd)"

BINARY="${SAYWRITE_HOST_SOURCE:-}"
if [ -z "${BINARY}" ]; then
    if [ -f "/app/libexec/saywrite-host" ]; then
        BINARY="/app/libexec/saywrite-host"
    else
        BINARY="${REPO_ROOT}/target/release/saywrite-host"
    fi
fi

if [ ! -f "${BINARY}" ]; then
    echo "Error: ${BINARY} not found. Build or bundle saywrite-host first." >&2
    exit 1
fi

SERVICE_TEMPLATE="${SAYWRITE_SERVICE_TEMPLATE:-}"
if [ -z "${SERVICE_TEMPLATE}" ]; then
    if [ -f "${SELF_DIR}/saywrite-host.service" ]; then
        SERVICE_TEMPLATE="${SELF_DIR}/saywrite-host.service"
    else
        SERVICE_TEMPLATE="${REPO_ROOT}/data/saywrite-host.service"
    fi
fi

DBUS_TEMPLATE="${SAYWRITE_DBUS_TEMPLATE:-}"
if [ -z "${DBUS_TEMPLATE}" ]; then
    if [ -f "${SELF_DIR}/io.github.saywrite.Host.service" ]; then
        DBUS_TEMPLATE="${SELF_DIR}/io.github.saywrite.Host.service"
    else
        DBUS_TEMPLATE="${REPO_ROOT}/data/io.github.saywrite.Host.service"
    fi
fi

run_user_systemctl() {
    if command -v systemctl >/dev/null 2>&1; then
        systemctl --user "$@"
    elif command -v flatpak-spawn >/dev/null 2>&1; then
        flatpak-spawn --host systemctl --user "$@"
    else
        echo "Warning: systemctl is unavailable; complete host startup manually." >&2
        return 127
    fi
}

echo "Installing saywrite-host companion daemon..."
mkdir -p "${BINDIR}" "${SYSTEMD_DIR}" "${DBUS_DIR}"
install -Dm755 "${BINARY}" "${BINDIR}/saywrite-host"

WHISPER_CLI="${SAYWRITE_WHISPER_SOURCE:-}"
if [ -z "${WHISPER_CLI}" ]; then
    for candidate in         "/app/bin/whisper-cli"         "${REPO_ROOT}/vendor/whisper.cpp/build/bin/whisper-cli"         "${REPO_ROOT}/vendor/whisper.cpp/build/bin/main"; do
        if [ -f "${candidate}" ]; then
            WHISPER_CLI="${candidate}"
            break
        fi
    done
fi

if [ -n "${WHISPER_CLI}" ] && [ -f "${WHISPER_CLI}" ]; then
    install -Dm755 "${WHISPER_CLI}" "${BINDIR}/whisper-cli"
    echo "  whisper-cli installed: ${BINDIR}/whisper-cli"
else
    echo "  Warning: whisper-cli not found; local transcription may be unavailable to the host."
fi

sed "s|ExecStart=.*|ExecStart=${BINDIR}/saywrite-host|"     "${SERVICE_TEMPLATE}" > "${SYSTEMD_DIR}/saywrite-host.service"
chmod 644 "${SYSTEMD_DIR}/saywrite-host.service"

sed "s|Exec=.*|Exec=${BINDIR}/saywrite-host|"     "${DBUS_TEMPLATE}" > "${DBUS_DIR}/io.github.saywrite.Host.service"
chmod 644 "${DBUS_DIR}/io.github.saywrite.Host.service"

run_user_systemctl daemon-reload || true
run_user_systemctl disable saywrite-host.service 2>/dev/null || true
run_user_systemctl unmask saywrite-host.service 2>/dev/null || true
run_user_systemctl start saywrite-host.service || true

echo
echo "SayWrite host companion is installed."
echo "  Daemon:    ${BINDIR}/saywrite-host"
echo "  Service:   ${SYSTEMD_DIR}/saywrite-host.service"
echo "  D-Bus:     ${DBUS_DIR}/io.github.saywrite.Host.service"
echo
echo "Useful commands:"
echo "  systemctl --user status saywrite-host"
echo "  journalctl --user -u saywrite-host -f"
