#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL_NAME="${1:-base.en}"
TARGET_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/saywrite/models"
DOWNLOADER="${ROOT_DIR}/vendor/whisper.cpp/models/download-ggml-model.sh"

mkdir -p "${TARGET_DIR}"

if [[ ! -x "${DOWNLOADER}" && ! -f "${DOWNLOADER}" ]]; then
  echo "whisper.cpp model downloader not found."
  echo "Run ./scripts/setup-whispercpp.sh first."
  exit 1
fi

echo "Downloading ${MODEL_NAME} to ${TARGET_DIR}"
bash "${DOWNLOADER}" "${MODEL_NAME}" "${TARGET_DIR}"

echo
echo "Model download complete."
echo "Expected file:"
echo "  ${TARGET_DIR}/ggml-${MODEL_NAME}.bin"
