#!/usr/bin/env bash
# Download the eego SDK vendor libraries with SHA-256 integrity verification.
#
# By default, fetches from the BrainFlow repository which ships all three
# binaries at third_party/ant_neuro/{linux,windows}/:
#   - libeego-SDK.so   (Linux x86_64)
#   - eego-SDK.dll     (Windows x64)
#   - eego-SDK32.dll   (Windows x86)
#
# Usage:
#   ./scripts/download-sdk.sh              # default: BrainFlow master
#   EEGO_SDK_BRANCH=v5.12.0 ./scripts/download-sdk.sh
#   EEGO_SDK_SOURCE=release EEGO_SDK_REPO=your-org/eego-sdk ./scripts/download-sdk.sh
#   EEGO_SDK_SKIP_HASH=1 ./scripts/download-sdk.sh   # skip integrity check
#
# Works on Linux, macOS, Windows (Git Bash / MSYS2 / WSL).

set -euo pipefail

SOURCE="${EEGO_SDK_SOURCE:-brainflow}"
BRANCH="${EEGO_SDK_BRANCH:-master}"
REPO="${EEGO_SDK_REPO:-brainflow-dev/brainflow}"
SKIP_HASH="${EEGO_SDK_SKIP_HASH:-0}"

# Known-good commit: last verified commit on BrainFlow master that contains
# the eego SDK binaries with matching SHA-256 hashes. Used as automatic
# fallback if master fails (e.g. file moved or branch force-pushed).
FALLBACK_COMMIT="f4953923a9737d0dcd6f76a0aecc3fa333431f06"

LIB_DIR="$(cd "$(dirname "$0")/.." && pwd)/lib"
mkdir -p "$LIB_DIR"

# ── Known-good SHA-256 hashes (eego SDK v1.3.29, build 57168) ────────────
# Uses a function instead of `declare -A` for Bash 3.x (macOS) compatibility.
expected_hash() {
    case "$1" in
        libeego-SDK.so)  echo "882867b584ceb52c5b12bc276430115ffb28c90b50884ee685073dfae473a94b" ;;
        eego-SDK.dll)    echo "fe22d1e754b9545340ed1f57a2a16b8ea01a199ac4bc8d28c6a09d3868be9809" ;;
        eego-SDK32.dll)  echo "c5b306edb7538cce03f81711c2282768f5221a7e41d593367889fbc7dbb660f2" ;;
        *)               echo "" ;;
    esac
}

# ── Cross-platform SHA-256 ────────────────────────────────────────────────
compute_sha256() {
    if command -v sha256sum &>/dev/null; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum &>/dev/null; then
        shasum -a 256 "$1" | cut -d' ' -f1
    elif command -v certutil &>/dev/null; then
        # Windows (cmd / PowerShell via Git Bash)
        certutil -hashfile "$1" SHA256 2>/dev/null | sed -n '2p' | tr -d ' '
    else
        echo ""
    fi
}

download() {
    local url="$1"
    local dest="$2"
    local name
    name="$(basename "$dest")"

    if [ -f "$dest" ]; then
        echo "  ✓ ${name} (already exists, $(du -h "$dest" | cut -f1))"
        verify_hash "$dest" "$name"
        return 0
    fi

    echo "  ↓ ${name}"
    if curl -fsSL -o "$dest" "$url" 2>/dev/null; then
        local bytes
        bytes="$(wc -c < "$dest" | tr -d ' ')"
        if [ "$bytes" -lt 10000 ]; then
            echo "  ✗ ${name} (received ${bytes} bytes — likely a 404 page)"
            rm -f "$dest"
            return 1
        fi
        echo "  ✓ ${name} ($(du -h "$dest" | cut -f1))"
        verify_hash "$dest" "$name"
    else
        echo "  ✗ ${name} (download failed)"
        rm -f "$dest"
        return 1
    fi
}

verify_hash() {
    local file="$1"
    local name="$2"

    if [ "$SKIP_HASH" = "1" ]; then
        return 0
    fi

    local expected
    expected="$(expected_hash "$name")"
    if [ -z "$expected" ]; then
        echo "    ⚠ no known hash for ${name} (skipping verification)"
        return 0
    fi

    local actual
    actual="$(compute_sha256 "$file")"
    if [ -z "$actual" ]; then
        echo "    ⚠ no sha256 tool available (skipping verification)"
        return 0
    fi

    if [ "$actual" = "$expected" ]; then
        echo "    🔒 SHA-256 verified: ${actual:0:16}..."
    else
        echo "    ⛔ SHA-256 MISMATCH!"
        echo "       expected: ${expected}"
        echo "       got:      ${actual}"
        echo "       The file may be corrupted or tampered with."
        echo "       Set EEGO_SDK_SKIP_HASH=1 to bypass this check."
        rm -f "$file"
        return 1
    fi
}

brainflow_url() {
    local ref="$1"
    local path="$2"
    echo "https://raw.githubusercontent.com/${REPO}/${ref}/third_party/ant_neuro/${path}"
}

# Try downloading from a given ref; returns 0 on success.
try_brainflow_download() {
    local ref="$1"
    local failed=0
    case "$(uname -s)" in
        Linux)
            download "$(brainflow_url "$ref" linux/libeego-SDK.so)" "$LIB_DIR/libeego-SDK.so" || failed=1
            ;;
        Darwin)
            echo "  ⚠ No macOS vendor library available."
            echo "    Use the native backend: cargo run -- --rate 500"
            ;;
        MINGW*|MSYS*|CYGWIN*|Windows_NT)
            download "$(brainflow_url "$ref" windows/eego-SDK.dll)" "$LIB_DIR/eego-SDK.dll" || failed=1
            download "$(brainflow_url "$ref" windows/eego-SDK32.dll)" "$LIB_DIR/eego-SDK32.dll" || failed=1
            ;;
        *)
            download "$(brainflow_url "$ref" linux/libeego-SDK.so)" "$LIB_DIR/libeego-SDK.so" || failed=1
            download "$(brainflow_url "$ref" windows/eego-SDK.dll)" "$LIB_DIR/eego-SDK.dll" || true
            download "$(brainflow_url "$ref" windows/eego-SDK32.dll)" "$LIB_DIR/eego-SDK32.dll" || true
            ;;
    esac
    return $failed
}

echo "Downloading eego SDK vendor libraries..."
echo "  Source: ${REPO}"
echo ""

if [ "$SOURCE" = "brainflow" ]; then
    echo "  Trying ${BRANCH}..."
    if ! try_brainflow_download "$BRANCH"; then
        echo ""
        echo "  ⚠ ${BRANCH} failed — falling back to known-good commit ${FALLBACK_COMMIT:0:12}..."
        echo ""
        try_brainflow_download "$FALLBACK_COMMIT"
    fi
else
    TAG="${EEGO_SDK_TAG:-latest}"
    if [ "$TAG" = "latest" ]; then
        BASE="https://github.com/${REPO}/releases/latest/download"
    else
        BASE="https://github.com/${REPO}/releases/download/${TAG}"
    fi
    case "$(uname -s)" in
        Linux)  download "${BASE}/libeego-SDK.so" "$LIB_DIR/libeego-SDK.so" ;;
        Darwin) echo "  ⚠ No macOS vendor library." ;;
        MINGW*|MSYS*|CYGWIN*|Windows_NT)
            download "${BASE}/eego-SDK.dll" "$LIB_DIR/eego-SDK.dll"
            download "${BASE}/eego-SDK32.dll" "$LIB_DIR/eego-SDK32.dll"
            ;;
        *)
            download "${BASE}/libeego-SDK.so" "$LIB_DIR/libeego-SDK.so" || true
            download "${BASE}/eego-SDK.dll" "$LIB_DIR/eego-SDK.dll" || true
            download "${BASE}/eego-SDK32.dll" "$LIB_DIR/eego-SDK32.dll" || true
            ;;
    esac
fi

echo ""
echo "Done. Libraries in: ${LIB_DIR}/"
ls -lh "$LIB_DIR"/*.so "$LIB_DIR"/*.dll 2>/dev/null || echo "  (no libraries found)"
