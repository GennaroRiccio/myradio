#!/usr/bin/env bash
#
# Build di release per Linux, Windows e macOS.
#
# Uso:
#   scripts/build-release.sh            # binario per il sistema corrente
#   scripts/build-release.sh --all      # nativo + cross per Windows (da Linux)
#   scripts/build-release.sh --windows  # solo cross per Windows (da Linux)
#
# I binari vengono copiati in dist/ con il nome myradio-<os>-<arch>[.exe].
#
# Prerequisiti:
#   - rustup (per aggiungere i target mancanti non installati)
#   - Linux: pkg-config e libasound2-dev (backend ALSA di rodio)
#   - cross per Windows (solo da Linux): sudo apt install -y mingw-w64
#   - macOS: solo build nativa; la cross da altri host non è supportata

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dist="$root/dist"

die() {
    echo "ERRORE: $*" >&2
    exit 1
}

host_os() {
    case "$(uname -s)" in
        Linux*) echo linux ;;
        Darwin*) echo macos ;;
        MINGW* | MSYS* | CYGWIN*) echo windows ;;
        *) die "sistema operativo non riconosciuto: $(uname -s)" ;;
    esac
}

host_arch() {
    case "$(uname -m)" in
        x86_64 | amd64) echo x86_64 ;;
        aarch64 | arm64) echo aarch64 ;;
        *) echo "$(uname -m)" ;;
    esac
}

exe_suffix() {
    [[ "$1" == windows ]] && echo ".exe" || echo ""
}

ensure_target() {
    local target="$1"
    if ! rustup target list --installed | grep -qx "$target"; then
        echo "> installo target rustup: $target"
        rustup target add "$target"
    fi
}

copy_binary() {
    local os="$1" source="$2"
    local suffix target_file
    suffix="$(exe_suffix "$os")"
    target_file="myradio-${os}-$(host_arch)${suffix}"
    mkdir -p "$dist"
    cp "$source" "$dist/$target_file"
    echo "> creato $dist/$target_file ($(du -h "$dist/$target_file" | cut -f1))"
}

native_build() {
    local os arch suffix
    os="$(host_os)"
    arch="$(host_arch)"
    echo "== Build nativo per $os/$arch =="
    cargo build --release --locked --manifest-path "$root/Cargo.toml"
    suffix="$(exe_suffix "$os")"
    copy_binary "$os" "$root/target/release/myradio$suffix"
}

windows_cross_build() {
    echo "== Build Windows (cross, GNU) =="
    [[ "$(host_os)" != linux ]] && \
        die "la cross-compilazione per Windows è supportata solo da Linux/mingw-w64"
    command -v x86_64-w64-mingw32-gcc >/dev/null \
        || die "mingw-w64 manca: installa con 'sudo apt install -y mingw-w64'"
    ensure_target x86_64-pc-windows-gnu
    cargo build --release --locked --target x86_64-pc-windows-gnu \
        --manifest-path "$root/Cargo.toml"
    copy_binary windows "$root/target/x86_64-pc-windows-gnu/release/myradio.exe"
}

mode="${1:-native}"
case "$mode" in
    native)
        native_build
        ;;
    --windows)
        windows_cross_build
        ;;
    --all)
        native_build
        if [[ "$(host_os)" == linux ]]; then
            windows_cross_build
        else
            echo "> cross per Windows saltata (solo da Linux)"
        fi
        ;;
    *)
        die "modalità sconosciuta: $mode (atteso: native | --windows | --all)"
        ;;
esac

echo "== Fatto: artefatti in $dist =="