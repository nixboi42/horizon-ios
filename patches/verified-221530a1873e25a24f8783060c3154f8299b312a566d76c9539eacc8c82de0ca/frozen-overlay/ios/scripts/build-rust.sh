#!/bin/sh
# Xcode pre-build phase: compile horizon-ios for the current Apple platform.
# Uses a workspace-local Rust installation when present, otherwise PATH.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$ROOT"

if [ -x "$ROOT/.local/cargo/bin/rustup" ]; then
  export CARGO_HOME="$ROOT/.local/cargo" RUSTUP_HOME="$ROOT/.local/rustup"
  PATH="$CARGO_HOME/bin:$PATH"; export PATH
fi
CARGO=${HORIZON_CARGO:-cargo}
if ! command -v "$CARGO" >/dev/null 2>&1; then
  echo "cargo not found. Install Rust and the iOS targets; see BUILDING.md." >&2
  exit 1
fi
# Xcode may replace PATH but preserves HORIZON_CARGO as an absolute path.
# Add its directory too so rustup finds the matching rustc proxy.
case "$CARGO" in
  */*)
    CARGO_BIN=$(dirname "$CARGO")
    PATH="$CARGO_BIN:$PATH"
    export PATH
    if [ -x "$CARGO_BIN/rustc" ]; then RUSTC="$CARGO_BIN/rustc"; export RUSTC; fi
    ;;
esac
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"

case "${PLATFORM_NAME:-iphoneos}" in
  iphoneos) TARGET=aarch64-apple-ios ;;
  iphonesimulator) TARGET=aarch64-apple-ios-sim ;;
  *) echo "Unsupported Apple platform: $PLATFORM_NAME" >&2; exit 1 ;;
esac
if command -v rustup >/dev/null 2>&1 && ! rustup target list --installed | grep -qx "$TARGET"; then
  echo "Rust target $TARGET is missing. Run: rustup target add $TARGET" >&2
  exit 1
fi

# An Xcode simulator SDKROOT must never leak into host build scripts.
unset SDKROOT
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-$(awk '$1 == "IPHONEOS_DEPLOYMENT_TARGET" {print $3}' "$ROOT/ios/Deployment.xcconfig")}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target/ios-$IPHONEOS_DEPLOYMENT_TARGET}"
# Release binaries embed panic locations and dependency source paths. Keep the
# build machine's user name and workspace layout out of the shipped app.
# Cargo's encoded form preserves flags containing spaces in checkout paths.
export CARGO_ENCODED_RUSTFLAGS=$(printf '%s\037%s\037%s'   "--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"   "--remap-path-prefix=$ROOT=/build/horizon-ios"   "--remap-path-prefix=$HOME=/build/user")
PROFILE=debug
case "${CONFIGURATION:-Debug}" in
Release|TestFlight)
  "$CARGO" build --locked -p horizon-ios --lib --target "$TARGET" --release
  PROFILE=release
  ;;
*)
  "$CARGO" build --locked -p horizon-ios --lib --target "$TARGET"
  ;;
esac
OUT="$ROOT/.local/ios-libs/${PLATFORM_NAME:-iphoneos}/${CONFIGURATION:-Debug}"
mkdir -p "$OUT"
cp "$CARGO_TARGET_DIR/$TARGET/$PROFILE/libhorizon_ios.a" "$OUT/libhorizon_ios.a"
