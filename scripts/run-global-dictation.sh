#!/usr/bin/env bash

set -euo pipefail

RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
LOCK_FILE="${RUNTIME_DIR}/saywrite-hotkey.lock"
STAMP_FILE="${RUNTIME_DIR}/saywrite-hotkey.last"
STATE_HOME="${XDG_STATE_HOME:-$HOME/.local/state}"
SESSION_MARKER="${STATE_HOME}/saywrite/host-session.pid"
NOW_MS="$(date +%s%3N)"
DEBOUNCE_MS=900

mkdir -p "${RUNTIME_DIR}"

exec 9>"${LOCK_FILE}"
if ! flock -n 9; then
  exit 0
fi

if [[ -f "${STAMP_FILE}" ]]; then
  LAST_MS="$(cat "${STAMP_FILE}" 2>/dev/null || echo 0)"
  if [[ "${LAST_MS}" =~ ^[0-9]+$ ]] && (( NOW_MS - LAST_MS < DEBOUNCE_MS )); then
    exit 0
  fi
fi

if [[ ! -f "${SESSION_MARKER}" ]]; then
  exit 0
fi

APP_PID="$(cat "${SESSION_MARKER}" 2>/dev/null || echo 0)"
if ! [[ "${APP_PID}" =~ ^[0-9]+$ ]] || ! kill -0 "${APP_PID}" 2>/dev/null; then
  exit 0
fi

printf '%s\n' "${NOW_MS}" > "${STAMP_FILE}"

busctl --user call \
  io.github.saywrite.Host \
  /io/github/saywrite/Host \
  io.github.saywrite.Host \
  ToggleDictation
