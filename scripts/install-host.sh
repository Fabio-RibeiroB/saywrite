#!/usr/bin/env bash
set -euo pipefail

BINDIR="${HOME}/.local/bin"
DBUS_DIR="${HOME}/.local/share/dbus-1/services"
SYSTEMD_DIR="${HOME}/.config/systemd/user"
SELF_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SELF_DIR}/.." && pwd)"

if [ -f "/.flatpak-info" ]; then
    APP_ID="$(awk -F= '/^name=/{print $2; exit}' /.flatpak-info)"
    if [ -z "${APP_ID}" ]; then
        echo "Error: could not determine Flatpak app ID from /.flatpak-info" >&2
        exit 1
    fi

    APP_LOCATION="$(flatpak-spawn --host flatpak info --show-location "${APP_ID}" 2>/dev/null || true)"
    if [ -z "${APP_LOCATION}" ]; then
        echo "Error: could not determine host Flatpak install location for ${APP_ID}" >&2
        exit 1
    fi

    HOST_ASSETS="${APP_LOCATION}/files"
    if ! flatpak-spawn --host test -f "${HOST_ASSETS}/libexec/saywrite-host"; then
        echo "Error: expected host asset not found: ${HOST_ASSETS}/libexec/saywrite-host" >&2
        exit 1
    fi

    HOST_INSTALL_OUTPUT="$(
        flatpak-spawn --host bash -s -- "${HOST_ASSETS}" <<'EOF'
set -euo pipefail

HOST_ASSETS="$1"
BINDIR="${HOME}/.local/bin"
DBUS_DIR="${HOME}/.local/share/dbus-1/services"
SYSTEMD_DIR="${HOME}/.config/systemd/user"

mkdir -p "${BINDIR}" "${DBUS_DIR}" "${SYSTEMD_DIR}"

install -Dm755 "${HOST_ASSETS}/libexec/saywrite-host" "${BINDIR}/saywrite-host"

if [ -f "${HOST_ASSETS}/bin/whisper-cli" ]; then
    install -Dm755 "${HOST_ASSETS}/bin/whisper-cli" "${BINDIR}/whisper-cli.bin"
    cat > "${BINDIR}/whisper-cli" <<EOF2
#!/usr/bin/env bash
set -euo pipefail
export LD_LIBRARY_PATH="${HOST_ASSETS}/lib64\${LD_LIBRARY_PATH:+:\${LD_LIBRARY_PATH}}"
exec "\$(dirname "\$0")/whisper-cli.bin" "\$@"
EOF2
    chmod 755 "${BINDIR}/whisper-cli"
fi

if [ -f "${HOST_ASSETS}/share/saywrite/run-global-dictation.sh" ]; then
    install -Dm755 \
        "${HOST_ASSETS}/share/saywrite/run-global-dictation.sh" \
        "${BINDIR}/saywrite-dictation.sh"
fi

sed "s|ExecStart=.*|ExecStart=${BINDIR}/saywrite-host|" \
    "${HOST_ASSETS}/share/saywrite/saywrite-host.service" > "${SYSTEMD_DIR}/saywrite-host.service"
chmod 644 "${SYSTEMD_DIR}/saywrite-host.service"

sed "s|Exec=.*|Exec=${BINDIR}/saywrite-host|" \
    "${HOST_ASSETS}/share/saywrite/io.github.saywrite.Host.service" > "${DBUS_DIR}/io.github.saywrite.Host.service"
chmod 644 "${DBUS_DIR}/io.github.saywrite.Host.service"

status_msg=""
if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload
    systemctl --user disable saywrite-host.service 2>/dev/null || true
    systemctl --user unmask saywrite-host.service 2>/dev/null || true
    if ! systemctl --user start saywrite-host.service; then
        status_msg="Warning: installed host files, but could not start saywrite-host.service."
    fi
fi

printf '%s\n' "${status_msg}"
EOF
    )"

    host_install_status=$?
    if [ "${host_install_status}" -ne 0 ]; then
        exit "${host_install_status}"
    fi

    if [ -n "${HOST_INSTALL_OUTPUT}" ]; then
        echo "${HOST_INSTALL_OUTPUT}" >&2
    fi

    echo
    echo "SayWrite host companion is installed on the host."
    echo "  Source: ${HOST_ASSETS}"
    echo "  Daemon: ~/.local/bin/saywrite-host"
    echo "  Service: ~/.config/systemd/user/saywrite-host.service"
    echo "  D-Bus: ~/.local/share/dbus-1/services/io.github.saywrite.Host.service"
    echo
    echo "Useful commands:"
    echo "  systemctl --user status saywrite-host"
    echo "  journalctl --user -u saywrite-host -f"
    exit 0
fi

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
