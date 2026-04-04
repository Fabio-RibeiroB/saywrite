#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="${ROOT_DIR}/vendor/whisper.cpp"
BUILD_DIR="${VENDOR_DIR}/build"
REPO_URL="https://github.com/ggml-org/whisper.cpp.git"

ACCELERATION="${1:-auto}"

if [[ ! -d "${VENDOR_DIR}/.git" ]]; then
  mkdir -p "${ROOT_DIR}/vendor"
  git clone --depth=1 "${REPO_URL}" "${VENDOR_DIR}"
else
  git -C "${VENDOR_DIR}" pull --ff-only
fi

cmake_flags=()

case "${ACCELERATION}" in
  auto)
    if command -v nvidia-smi >/dev/null 2>&1; then
      cmake_flags+=("-DGGML_CUDA=1")
      mode="cuda"
    elif pkg-config --exists vulkan; then
      cmake_flags+=("-DGGML_VULKAN=1")
      mode="vulkan"
    else
      mode="cpu"
    fi
    ;;
  cuda)
    cmake_flags+=("-DGGML_CUDA=1")
    mode="cuda"
    ;;
  vulkan)
    cmake_flags+=("-DGGML_VULKAN=1")
    mode="vulkan"
    ;;
  cpu)
    mode="cpu"
    ;;
  *)
    echo "Unknown acceleration mode: ${ACCELERATION}"
    echo "Use one of: auto, cuda, vulkan, cpu"
    exit 1
    ;;
esac

echo "Building whisper.cpp with mode: ${mode}"

cmake -S "${VENDOR_DIR}" -B "${BUILD_DIR}" -G Ninja "${cmake_flags[@]}"
cmake --build "${BUILD_DIR}" -j

echo
echo "whisper.cpp built successfully."
echo "Binary should be at:"
echo "  ${BUILD_DIR}/bin/whisper-cli"
echo
echo "You can point SayWrite at it automatically with:"
echo "  export SAYWRITE_WHISPER_CLI=${BUILD_DIR}/bin/whisper-cli"
