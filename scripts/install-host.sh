#!/usr/bin/env bash
set -euo pipefail

BINDIR="${HOME}/.local/bin"
DBUS_DIR="${HOME}/.local/share/dbus-1/services"
SYSTEMD_DIR="${HOME}/.config/systemd/user"

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${SCRIPT_DIR}/target/release/saywrite-host"

if [ ! -f "${BINARY}" ]; then
    echo "Error: ${BINARY} not found. Run 'cargo build --release' first." >&2
    exit 1
fi

echo "Installing saywrite-host companion daemon..."

install -Dm755 "${BINARY}" "${BINDIR}/saywrite-host"

mkdir -p "${SYSTEMD_DIR}"
sed "s|ExecStart=.*|ExecStart=${BINDIR}/saywrite-host|" \
    "${SCRIPT_DIR}/data/saywrite-host.service" > "${SYSTEMD_DIR}/saywrite-host.service"
chmod 644 "${SYSTEMD_DIR}/saywrite-host.service"

mkdir -p "${DBUS_DIR}"
sed "s|Exec=.*|Exec=${BINDIR}/saywrite-host|" \
    "${SCRIPT_DIR}/data/io.github.saywrite.Host.service" > "${DBUS_DIR}/io.github.saywrite.Host.service"
chmod 644 "${DBUS_DIR}/io.github.saywrite.Host.service"

systemctl --user daemon-reload
# Do not enable — the GUI starts and stops the daemon on launch/exit.
# Disable auto-start in case this is a reinstall over an older version.
systemctl --user disable saywrite-host.service 2>/dev/null || true
systemctl --user unmask saywrite-host.service 2>/dev/null || true
systemctl --user start saywrite-host.service

echo
echo "SayWrite host companion is installed."
echo "  Binary: ${BINDIR}/saywrite-host"
echo "  User service: ${SYSTEMD_DIR}/saywrite-host.service"
echo "  D-Bus activation: ${DBUS_DIR}/io.github.saywrite.Host.service"
echo
echo "Host status:"
systemctl --user --no-pager --full status saywrite-host.service | sed -n '1,8p'
echo
echo "Useful commands:"
echo "  systemctl --user status saywrite-host"
echo "  journalctl --user -u saywrite-host -f"

if [[ "${XDG_CURRENT_DESKTOP:-}" == *GNOME* ]] || [[ "${XDG_CURRENT_DESKTOP:-}" == *gnome* ]]; then
    echo
    echo "Optional GNOME shortcut fallback:"
    echo "  bash scripts/install-gnome-shortcut.sh"
fi
