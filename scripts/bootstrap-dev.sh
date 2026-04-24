#!/usr/bin/env bash

set -euo pipefail

if [[ "${EUID}" -eq 0 ]]; then
  echo "Run this script as your normal user, not as root."
  exit 1
fi

if ! command -v sudo >/dev/null 2>&1; then
  echo "sudo is required for installing local development packages."
  exit 1
fi

PACKAGES=(
  cmake
  ninja-build
  git
  pkg-config
  ffmpeg
  libgstreamer1.0-dev
  libgstreamer-plugins-base1.0-dev
  libvulkan-dev
  vulkan-tools
)

echo "SayWrite development bootstrap"
echo
echo "This installs host packages needed for local development only."
echo "End users should get these through the native package dependencies."
echo
echo "Packages:"
printf '  - %s\n' "${PACKAGES[@]}"
echo

sudo apt-get update
sudo apt-get install -y "${PACKAGES[@]}"

echo
echo "Development dependencies installed."
echo "Next step: run cargo check or build the native package."
