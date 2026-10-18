#!/bin/bash
# OpenStorage installer for macOS and Linux.
#   curl -fsSL https://openstorage.mdnlabs.dev/install.sh | bash
#
# Downloads the latest release from GitHub with curl (so macOS does not tag it as "downloaded from the
# internet"), installs it and opens it. Read it first if you like: it only fetches a release asset from
# github.com/MarshallNickolauson/OpenStorage and copies it into place.
set -euo pipefail

BASE="https://github.com/MarshallNickolauson/OpenStorage/releases/latest/download"
os="$(uname -s)"; arch="$(uname -m)"
tmp="$(mktemp -d)"; mnt=""
cleanup() { [ -n "$mnt" ] && hdiutil detach -quiet "$mnt" 2>/dev/null || true; rm -rf "$tmp"; }
trap cleanup EXIT

case "$os" in
  Darwin)
    [ "$arch" = "arm64" ] && name="openstorage-macos-arm64.dmg" || name="openstorage-macos-intel.dmg"
    echo "Downloading OpenStorage..."
    curl -fL --progress-bar -o "$tmp/os.dmg" "$BASE/$name" || { echo "Could not download $name. See https://openstorage.mdnlabs.dev/download" >&2; exit 1; }
    mnt="$(hdiutil attach -nobrowse -readonly -noautoopen "$tmp/os.dmg" | awk -F'\t' '/\/Volumes\//{print $NF}' | tail -1)"
    [ -d "$mnt/OpenStorage.app" ] || { echo "The disk image did not contain OpenStorage.app." >&2; exit 1; }
    osascript -e 'tell application "OpenStorage" to quit' >/dev/null 2>&1 || true
    dest="/Applications"; [ -w "$dest" ] || { dest="$HOME/Applications"; mkdir -p "$dest"; }
    rm -rf "$dest/OpenStorage.app"; cp -R "$mnt/OpenStorage.app" "$dest/"
    xattr -dr com.apple.quarantine "$dest/OpenStorage.app" 2>/dev/null || true
    echo "Installed to $dest/OpenStorage.app"
    open "$dest/OpenStorage.app"
    ;;
  Linux)
    [ "$arch" = "x86_64" ] || { echo "Only x86_64 Linux builds are published. Build from source: https://github.com/MarshallNickolauson/OpenStorage" >&2; exit 1; }
    echo "Downloading OpenStorage..."
    curl -fL --progress-bar -o "$tmp/os.tar.gz" "$BASE/openstorage-linux-x64.tar.gz"
    tar -xzf "$tmp/os.tar.gz" -C "$tmp"
    dest="$HOME/.local/bin"; mkdir -p "$dest"
    install -m 755 "$tmp/openstorage" "$dest/openstorage"
    echo "Installed to $dest/openstorage"
    case ":$PATH:" in *":$dest:"*) ;; *) echo "Add $dest to your PATH to run it as: openstorage" ;; esac
    ;;
  *) echo "Unsupported system: $os. Windows: irm https://openstorage.mdnlabs.dev/install.ps1 | iex" >&2; exit 1 ;;
esac
