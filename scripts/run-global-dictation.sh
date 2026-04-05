#!/usr/bin/env bash

set -euo pipefail

busctl --user call \
  io.github.saywrite.Host \
  /io/github/saywrite/Host \
  io.github.saywrite.Host \
  ToggleDictation
