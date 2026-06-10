#!/usr/bin/env bash
# build-platforms.sh — build & run OpenFang (ChaiTex fork) for every supported platform.
#
# Usage:   ./scripts/build-platforms.sh <target> [--fast]
# Help:    ./scripts/build-platforms.sh help
#
# Targets:
#   native       Build for the machine you are on (auto-detect)
#   linux        Linux x86_64 (glibc)            x86_64-unknown-linux-gnu
#   linux-musl   Linux x86_64 (static, portable) x86_64-unknown-linux-musl
#   windows      Windows x86_64                  x86_64-pc-windows-gnu (cross) / msvc (native)
#   macos        macOS Apple Silicon             aarch64-apple-darwin
#   macos-intel  macOS Intel                     x86_64-apple-darwin
#   pc162        PocketClaw / pc162 (Armbian)    aarch64-unknown-linux-gnu
#   rpi          Raspberry Pi 3/4/5 (64-bit OS)  aarch64-unknown-linux-gnu
#   rpi-32       Raspberry Pi 2 / 32-bit OS      armv7-unknown-linux-gnueabihf
#   opi5         Orange Pi 5 / RK3588 (Armbian)  aarch64-unknown-linux-gnu
#
# Flags:
#   --fast       Use the `release-fast` cargo profile (quicker compile, slightly
#                larger/slower binary). Default is `release` (LTO, stripped, ~32 MB).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PROFILE="release"
PROFILE_FLAG="--release"
ARGS=()
for a in "$@"; do
  case "$a" in
    --fast) PROFILE="release-fast"; PROFILE_FLAG="--profile release-fast" ;;
    *) ARGS+=("$a") ;;
  esac
done
TARGET="${ARGS[0]:-help}"

bold() { printf '\033[1m%s\033[0m\n' "$*"; }

have() { command -v "$1" >/dev/null 2>&1; }

# Cross-compile with `cross` (Docker-based) when available, else plain cargo
# with an installed rustup target. Cross.toml already installs libssl for the
# ARM targets inside the cross containers.
build_cross() { # $1 = rust target triple
  local triple="$1"
  if have cross; then
    bold "→ cross build $PROFILE_FLAG --target $triple -p openfang-cli"
    cross build $PROFILE_FLAG --target "$triple" -p openfang-cli
  else
    echo "‼ 'cross' not found — falling back to cargo (needs a working C cross-toolchain)."
    echo "  Install cross (recommended):  cargo install cross --git https://github.com/cross-rs/cross"
    rustup target add "$triple"
    bold "→ cargo build $PROFILE_FLAG --target $triple -p openfang-cli"
    cargo build $PROFILE_FLAG --target "$triple" -p openfang-cli
  fi
  bold "✓ Binary: target/$triple/$PROFILE/openfang"
}

build_native() {
  bold "→ cargo build $PROFILE_FLAG -p openfang-cli"
  cargo build $PROFILE_FLAG -p openfang-cli
  local ext=""
  [[ "${OS:-}" == "Windows_NT" ]] && ext=".exe"
  bold "✓ Binary: target/$PROFILE/openfang$ext"
}

print_run_help() {
  cat <<'EOF'

──────────────────────────── RUNNING OPENFANG ────────────────────────────
First run (creates ~/.openfang/config.toml, interactive provider setup):
    openfang init

Start the daemon (needs at least one LLM provider API key):
    GROQ_API_KEY=<key> openfang start          # Linux / macOS / Armbian
    $env:GROQ_API_KEY="<key>"; openfang start  # Windows PowerShell

Verify it is up:
    curl http://127.0.0.1:4200/api/health
Dashboard:  http://127.0.0.1:4200
Diagnostics: openfang doctor      Status: openfang status

Stop:  Ctrl+C in foreground, or kill the process:
    Linux/macOS:  pkill openfang
    Windows:      taskkill //PID <pid> //F      (Git Bash: double slashes)
───────────────────────────────────────────────────────────────────────────
EOF
}

print_help() {
  cat <<'EOF'
═══════════════════════════════════════════════════════════════════════════
 OpenFang (ChaiTex) — build & run guide
═══════════════════════════════════════════════════════════════════════════
Prereqs (all platforms): Rust stable toolchain via https://rustup.rs
The workspace pins the toolchain in rust-toolchain.toml (stable + clippy/rustfmt).

USAGE
    ./scripts/build-platforms.sh <target> [--fast]

    --fast   release-fast profile: thin-LTO, parallel codegen — ~2-3× faster
             compiles for day-to-day testing. Ship real releases without it.

───────────────────────────────────────────────────────────────────────────
 LINUX (x86_64)                          ./scripts/build-platforms.sh linux
───────────────────────────────────────────────────────────────────────────
    sudo apt install build-essential pkg-config libssl-dev   # Debian/Ubuntu
    cargo build --release -p openfang-cli
    # → target/release/openfang

    Fully static binary (runs on any distro, no glibc dependency):
    ./scripts/build-platforms.sh linux-musl
    # → target/x86_64-unknown-linux-musl/release/openfang

───────────────────────────────────────────────────────────────────────────
 WINDOWS (x86_64)                      ./scripts/build-platforms.sh windows
───────────────────────────────────────────────────────────────────────────
    Native (recommended): install Rust + "Desktop development with C++"
    (MSVC Build Tools), then in PowerShell or Git Bash:
        cargo build --release -p openfang-cli
        # → target\release\openfang.exe

    From Linux/macOS (cross): uses x86_64-pc-windows-gnu via `cross`.
    NOTE: if the daemon is already running, openfang.exe is locked —
    kill it first or build with `cargo build --workspace --lib`.

───────────────────────────────────────────────────────────────────────────
 MACOS                                   ./scripts/build-platforms.sh macos
───────────────────────────────────────────────────────────────────────────
    xcode-select --install        # Command Line Tools
    Apple Silicon (M1+):  ./scripts/build-platforms.sh macos
    Intel:                ./scripts/build-platforms.sh macos-intel
    # → target/<triple>/release/openfang
    Cross-compiling *to* macOS only works on a Mac (Apple SDK licence);
    on a Mac you can build both arches and `lipo` them into a universal binary.

───────────────────────────────────────────────────────────────────────────
 ARMBIAN — pc162 / PocketClaw            ./scripts/build-platforms.sh pc162
───────────────────────────────────────────────────────────────────────────
    Device: Allwinner quad-core, 1.9 GB RAM, aarch64, Armbian (Ubuntu 24.04).
    DO NOT compile on the device — 1.9 GB RAM is not enough for LTO.
    Cross-compile from your workstation (needs Docker for `cross`):

        cargo install cross --git https://github.com/cross-rs/cross
        ./scripts/build-platforms.sh pc162
        # → target/aarch64-unknown-linux-gnu/release/openfang

    Deploy:
        scp target/aarch64-unknown-linux-gnu/release/openfang user@pc162:~/bin/
        ssh user@pc162 'GROQ_API_KEY=<key> ~/bin/openfang start'

    Run as a service (survives reboot; recommended on pocketclaw):
        ssh user@pc162 'sudo tee /etc/systemd/system/openfang.service' <<UNIT
        [Unit]
        Description=OpenFang Agent OS
        After=network-online.target
        [Service]
        ExecStart=%h/bin/openfang start
        Environment=GROQ_API_KEY=<key>
        Restart=on-failure
        MemoryMax=1200M
        [Install]
        WantedBy=default.target
        UNIT
        ssh user@pc162 'sudo systemctl daemon-reload && sudo systemctl enable --now openfang'

    pc162 tuning (in ~/.openfang/config.toml):
        [reasoning] fts_backfill = "lazy"   # SD-card friendly
        keep reasoning levels at Medium or below (Minimal/Low skip the LLM)

───────────────────────────────────────────────────────────────────────────
 ARMBIAN / RASPBERRY PI OS                 ./scripts/build-platforms.sh rpi
───────────────────────────────────────────────────────────────────────────
    Pi 3/4/5 with 64-bit OS:   rpi      → aarch64-unknown-linux-gnu
    Pi 2 or 32-bit OS:         rpi-32   → armv7-unknown-linux-gnueabihf
    Pi 5 (8 GB) can also build natively (slow, ~40 min):
        sudo apt install build-essential pkg-config libssl-dev
        cargo build --release -p openfang-cli

───────────────────────────────────────────────────────────────────────────
 ORANGE PI 5 (RK3588, Armbian)            ./scripts/build-platforms.sh opi5
───────────────────────────────────────────────────────────────────────────
    Same aarch64 triple as pc162/rpi — one binary serves all three.
    The RK3588 with 8-16 GB RAM is fast enough to build natively too:
        sudo apt install build-essential pkg-config libssl-dev
        cargo build --release -p openfang-cli

───────────────────────────────────────────────────────────────────────────
 DOCKER (any platform)
───────────────────────────────────────────────────────────────────────────
    docker compose up -d          # uses the repo Dockerfile, port 4200
EOF
  print_run_help
}

case "$TARGET" in
  help|-h|--help) print_help ;;
  native)        build_native; print_run_help ;;
  linux)         build_cross x86_64-unknown-linux-gnu ;;
  linux-musl)    build_cross x86_64-unknown-linux-musl ;;
  windows)
    if [[ "${OS:-}" == "Windows_NT" ]]; then build_native
    else build_cross x86_64-pc-windows-gnu; fi ;;
  macos)
    if [[ "$(uname -s)" == "Darwin" ]]; then
      rustup target add aarch64-apple-darwin
      cargo build $PROFILE_FLAG --target aarch64-apple-darwin -p openfang-cli
      bold "✓ Binary: target/aarch64-apple-darwin/$PROFILE/openfang"
    else
      echo "✗ macOS builds require a Mac (Apple SDK licensing). See: $0 help"; exit 1
    fi ;;
  macos-intel)
    if [[ "$(uname -s)" == "Darwin" ]]; then
      rustup target add x86_64-apple-darwin
      cargo build $PROFILE_FLAG --target x86_64-apple-darwin -p openfang-cli
      bold "✓ Binary: target/x86_64-apple-darwin/$PROFILE/openfang"
    else
      echo "✗ macOS builds require a Mac (Apple SDK licensing). See: $0 help"; exit 1
    fi ;;
  pc162|rpi|opi5) build_cross aarch64-unknown-linux-gnu ;;
  rpi-32)         build_cross armv7-unknown-linux-gnueabihf ;;
  *) echo "Unknown target: $TARGET"; echo "Run: $0 help"; exit 1 ;;
esac
