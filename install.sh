#!/bin/sh
# OpenPrototype installer for Linux and macOS.
#
# Downloads the matching release binary, optionally fetches the original
# Prototype CD image from the Internet Archive, and runs the binary's own
# offline `install` to place the launcher entry and the disc. Re-run any time
# to update to the latest release.
#
#   curl -fsSL https://raw.githubusercontent.com/openprototype-game/openprototype/main/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --cue /path/to/PROTOTYPE.cue

set -eu

REPO="openprototype-game/openprototype"
ARCHIVE_ITEM="prototype-1995"
DISC_BASE="https://archive.org/download/${ARCHIVE_ITEM}"
DISC_BIN_SHA1="e3054ebb69f9cc8810d96822348818712476d06c"
DISC_CUE_SHA1="b68b02d2313bb070087dd19263cf9164186d3fb0"

cue=""
assume_yes=0

usage() {
    cat <<'EOF'
OpenPrototype installer.

Usage: install.sh [--cue PATH] [--yes]

  --cue PATH   Install from an existing PROTOTYPE.cue instead of downloading
               the disc image.
  --yes        Don't prompt; download the disc image if none is supplied.
  --help       Show this message.
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --cue) cue="${2:?--cue needs a path}"; shift 2 ;;
        --cue=*) cue="${1#--cue=}"; shift ;;
        --yes|-y) assume_yes=1; shift ;;
        --help|-h) usage; exit 0 ;;
        *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

os="$(uname -s)"
arch="$(uname -m)"

case "${os}-${arch}" in
    Linux-x86_64) target="x86_64-unknown-linux-gnu" ;;
    Darwin-x86_64) target="x86_64-apple-darwin" ;;
    Darwin-arm64) target="aarch64-apple-darwin" ;;
    *)
        echo "No prebuilt binary for ${os} ${arch}." >&2
        echo "Build from source instead: https://github.com/${REPO}" >&2
        exit 1
        ;;
esac

have() {
    command -v "$1" >/dev/null 2>&1
}

fetch() {
    if have curl; then
        curl -fL --progress-bar "$1" -o "$2"
    elif have wget; then
        wget -q --show-progress -O "$2" "$1"
    else
        echo "Need curl or wget to download." >&2
        exit 1
    fi
}

sha1_of() {
    if have sha1sum; then
        sha1sum "$1" | cut -d' ' -f1
    elif have shasum; then
        shasum -a 1 "$1" | cut -d' ' -f1
    fi
}

verify() {
    actual="$(sha1_of "$1")"

    if [ -z "$actual" ]; then
        echo "  (no sha1 tool found; skipping the integrity check)" >&2
        return 0
    fi

    if [ "$actual" != "$2" ]; then
        echo "Checksum mismatch for $1" >&2
        echo "  expected $2" >&2
        echo "  got      $actual" >&2
        exit 1
    fi
}

workdir="$(mktemp -d)"

cleanup() {
    rm -rf "$workdir"
}

trap cleanup EXIT INT TERM

asset="openprototype-${target}.tar.gz"
echo "Downloading ${asset} ..."
fetch "https://github.com/${REPO}/releases/latest/download/${asset}" "${workdir}/${asset}"
tar -xzf "${workdir}/${asset}" -C "$workdir"

binary="${workdir}/openprototype"

if [ ! -x "$binary" ]; then
    echo "The release archive did not contain the openprototype binary." >&2
    exit 1
fi

if [ -z "$cue" ]; then
    echo
    echo "OpenPrototype needs the original Prototype CD image (about 270 MB)."
    echo "It is preserved at the Internet Archive:"
    echo "  https://archive.org/details/${ARCHIVE_ITEM}"
    echo

    if [ "$assume_yes" -ne 1 ]; then
        if [ ! -r /dev/tty ]; then
            echo "No terminal to prompt at. Re-run with --yes to download it," >&2
            echo "or with --cue /path/to/PROTOTYPE.cue if you already have it." >&2
            exit 1
        fi

        printf "Download it now? [Y/n] "
        read -r answer < /dev/tty || answer="n"

        case "$answer" in
            [Nn]*)
                echo "Re-run with --cue /path/to/PROTOTYPE.cue once you have the disc."
                exit 0
                ;;
        esac
    fi

    echo "Downloading the disc image ..."
    fetch "${DISC_BASE}/PROTOTYPE.bin" "${workdir}/PROTOTYPE.bin"
    verify "${workdir}/PROTOTYPE.bin" "$DISC_BIN_SHA1"
    fetch "${DISC_BASE}/PROTOTYPE.cue" "${workdir}/PROTOTYPE.cue"
    verify "${workdir}/PROTOTYPE.cue" "$DISC_CUE_SHA1"
    cue="${workdir}/PROTOTYPE.cue"
fi

echo
echo "Installing ..."
"$binary" install --cue "$cue"
