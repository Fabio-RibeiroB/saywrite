#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID:-$(id -u)} -eq 0 ]]; then
  echo "Run this script as your normal user. It will call sudo only when needed."
  exit 1
fi

sudo apt-get update
sudo apt-get install -y \
  cargo \
  rustc \
  pkg-config \
  libgtk-4-dev \
  libadwaita-1-dev \
  libglib2.0-dev \
  blueprint-compiler

echo
echo "Rust/GTK development dependencies are installed."
echo "Next step:"
echo "  cargo run"
