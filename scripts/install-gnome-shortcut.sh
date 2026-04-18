#!/usr/bin/env bash

set -euo pipefail

SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HANDS_FREE_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/saywrite-hands-free/"
HANDS_FREE_COMMAND="${SELF_DIR}/run-global-dictation.sh"
LEGACY_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/saywrite/"
OLD_QUICK_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/saywrite-quick/"
SHORTCUT_LABEL="${1:-Super+Alt+D}"
BINDING="$(
  python3 - <<'PY' "${SHORTCUT_LABEL}"
import sys

shortcut = sys.argv[1].strip()
parts = [part for part in shortcut.split("+") if part]
if not parts:
    print("<Super><Alt>d")
    raise SystemExit(0)

modifiers = {
    "super": "<Super>",
    "alt": "<Alt>",
    "ctrl": "<Primary>",
    "control": "<Primary>",
    "shift": "<Shift>",
}

binding = []
key = None
for part in parts:
    token = part.strip()
    lower = token.lower()
    if lower in modifiers:
        binding.append(modifiers[lower])
    else:
        key = token.lower()

if not key:
    key = "d"

if key == "space":
    key = "space"

print("".join(binding) + key)
PY
)"

# Replace SayWrite-owned entries deterministically so stale bindings do not survive.
CURRENT="$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)"
FILTERED="$(
  python3 - <<'PY' "${CURRENT}" "${HANDS_FREE_PATH}" "${LEGACY_PATH}" "${OLD_QUICK_PATH}"
import ast
import sys

raw = sys.argv[1]
keep = []
drop = set(sys.argv[2:])
try:
    paths = ast.literal_eval(raw)
except Exception:
    paths = []
for path in paths:
    if path not in drop:
        keep.append(path)
keep.append(sys.argv[2])
print(repr(keep))
PY
)"

gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "${FILTERED}"

gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:${HANDS_FREE_PATH} name "SayWrite Hands-Free Dictation"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:${HANDS_FREE_PATH} command "${HANDS_FREE_COMMAND}"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:${HANDS_FREE_PATH} binding "${BINDING}"

echo "Installed GNOME shortcut:"
echo "  ${SHORTCUT_LABEL} -> ${HANDS_FREE_COMMAND}"
