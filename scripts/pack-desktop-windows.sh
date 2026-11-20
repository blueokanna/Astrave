#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN=""
VERSION=""
OUTDIR="${ROOT}/dist"
ARCH_LABEL="amd64"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin) BIN="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --outdir) OUTDIR="$2"; shift 2 ;;
    --arch) ARCH_LABEL="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "$VERSION" ]]; then
  package_id="$(cargo pkgid --manifest-path "${ROOT}/Cargo.toml" -p astrave)"
  VERSION="${package_id##*@}"
  VERSION="${VERSION##*#}"
fi

if [[ -z "$BIN" ]]; then
  arch=x64
  [[ "$ARCH_LABEL" == arm64 ]] && arch=arm64
  BIN="${ROOT}/app/build/windows/${arch}/runner/Release/astrave_desktop.exe"
fi

if [[ ! -f "$BIN" ]]; then
  echo "binary not found: $BIN" >&2
  exit 1
fi

# Relative --outdir is resolved from repo root; always use an absolute path
# so PowerShell Compress-Archive does not resolve against the staging dir.
case "$OUTDIR" in
  /*|[A-Za-z]:*) ;;
  *) OUTDIR="${ROOT}/${OUTDIR}" ;;
esac
mkdir -p "$OUTDIR"
OUTDIR="$(cd "$OUTDIR" && pwd)"

to_win_path() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -w "$1"
  else
    # Git Bash / MSYS fallback: /d/foo -> D:\foo
    local p="$1"
    if [[ "$p" =~ ^/([a-zA-Z])/(.*)$ ]]; then
      local drive
      drive="$(printf '%s' "${BASH_REMATCH[1]}" | tr '[:lower:]' '[:upper:]')"
      echo "${drive}:\\${BASH_REMATCH[2]//\//\\}"
    else
      echo "${p//\//\\}"
    fi
  fi
}

STAGE="${OUTDIR}/.win-stage-$$"
rm -rf "$STAGE"
mkdir -p "$STAGE"

cp -R "$(dirname "$BIN")/." "$STAGE/"
cp "${ROOT}/LICENSE" "$STAGE/"

ZIP_NAME="astrave-desktop_${VERSION}_windows_${ARCH_LABEL}.zip"
ZIP_PATH="${OUTDIR}/${ZIP_NAME}"
rm -f "$ZIP_PATH"

if command -v zip >/dev/null 2>&1; then
  (
    cd "$STAGE"
    zip -q -r "$ZIP_PATH" .
  )
elif command -v powershell.exe >/dev/null 2>&1; then
  STAGE_WIN="$(to_win_path "$STAGE")"
  ZIP_WIN="$(to_win_path "$ZIP_PATH")"
  powershell.exe -NoProfile -Command \
    "Compress-Archive -Path '${STAGE_WIN}\\*' -DestinationPath '${ZIP_WIN}' -Force"
elif command -v powershell >/dev/null 2>&1; then
  STAGE_WIN="$(to_win_path "$STAGE")"
  ZIP_WIN="$(to_win_path "$ZIP_PATH")"
  powershell -NoProfile -Command \
    "Compress-Archive -Path '${STAGE_WIN}\\*' -DestinationPath '${ZIP_WIN}' -Force"
else
  echo "need zip or powershell to create archive" >&2
  exit 1
fi

rm -rf "$STAGE"
echo "wrote ${ZIP_PATH}"
ls -lh "$ZIP_PATH"
