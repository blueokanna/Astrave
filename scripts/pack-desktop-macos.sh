#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${ROOT}/app/build/macos/Build/Products/Release/Astrave.app"
VERSION=""
OUTDIR="${ROOT}/dist"
APP_NAME="Astrave Desktop"
ARCH_LABEL=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin) BIN="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --outdir) OUTDIR="$2"; shift 2 ;;
    --name) APP_NAME="$2"; shift 2 ;;
    --arch) ARCH_LABEL="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "$VERSION" ]]; then
  package_id="$(cargo pkgid --manifest-path "${ROOT}/Cargo.toml" -p astrave-desktop)"
  VERSION="${package_id##*@}"
  VERSION="${VERSION##*#}"
fi

if [[ -z "$ARCH_LABEL" ]]; then
  case "$(uname -m)" in
    arm64|aarch64) ARCH_LABEL="arm64" ;;
    x86_64|amd64) ARCH_LABEL="amd64" ;;
    *) ARCH_LABEL="$(uname -m)" ;;
  esac
fi

if [[ ! -d "$BIN" ]]; then
  echo "binary not found: $BIN" >&2
  echo "build first: cd app && flutter build macos" >&2
  exit 1
fi

mkdir -p "$OUTDIR"
APP_DIR="${OUTDIR}/${APP_NAME}.app"
ditto "$BIN" "$APP_DIR"
STAGE="${OUTDIR}/.dmg-stage-$$"
DMG_NAME="astrave-desktop_${VERSION}_darwin_${ARCH_LABEL}.dmg"
DMG_PATH="${OUTDIR}/${DMG_NAME}"
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp -R "$APP_DIR" "$STAGE/"
cp "${ROOT}/LICENSE" "$STAGE/LICENSE"
ln -s /Applications "${STAGE}/Applications"

rm -f "$DMG_PATH"
hdiutil create \
  -volname "${APP_NAME}" \
  -srcfolder "$STAGE" \
  -ov \
  -format UDZO \
  "$DMG_PATH" >/dev/null
rm -rf "$STAGE"

echo "wrote ${DMG_PATH}"
ls -lh "$APP_DIR" "$DMG_PATH"
