#!/bin/sh

set -eu

REPOSITORY="ziward-inc/auq-wizard"
RELEASE_API="https://api.github.com/repos/$REPOSITORY/releases/latest"
APP_NAME="auq-wizard.app"
BUNDLE_IDENTIFIER="com.ziward.auq-wizard"
INSTALL_ROOT=${AUQ_INSTALL_DIR:-"$HOME/Applications"}
DESTINATION="$INSTALL_ROOT/$APP_NAME"

fail() {
  printf 'auq-wizard installer: %s\n' "$1" >&2
  exit 1
}

fetch_stdout() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- "$1"
  else
    fail "curl or wget is required"
  fi
}

fetch_file() {
  source_url=$1
  output_path=$2
  if command -v curl >/dev/null 2>&1; then
    curl -fL --retry 3 --progress-bar "$source_url" -o "$output_path"
  elif command -v wget >/dev/null 2>&1; then
    wget --progress=bar:force:noscroll -O "$output_path" "$source_url"
  else
    fail "curl or wget is required"
  fi
}

read_bundle_identifier() {
  /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$1/Contents/Info.plist" 2>/dev/null || true
}

[ "$(uname -s)" = "Darwin" ] || fail "only macOS is currently supported"
[ "$(uname -m)" = "arm64" ] || fail "the current release supports Apple Silicon Macs only"
command -v hdiutil >/dev/null 2>&1 || fail "hdiutil is required"
command -v shasum >/dev/null 2>&1 || fail "shasum is required"

release_json=$(fetch_stdout "$RELEASE_API") || fail "could not read the latest GitHub Release"
dmg_url=$(printf '%s\n' "$release_json" | awk -F '"' '/browser_download_url/ && /_aarch64\.dmg"/ { print $4; exit }')
checksum_url=$(printf '%s\n' "$release_json" | awk -F '"' '/browser_download_url/ && /_aarch64\.dmg\.sha256"/ { print $4; exit }')
[ -n "$dmg_url" ] || fail "the latest release does not contain an Apple Silicon DMG"
[ -n "$checksum_url" ] || fail "the latest release does not contain a DMG checksum"

mkdir -p "$INSTALL_ROOT"
work_directory=$(mktemp -d "$INSTALL_ROOT/.auq-wizard-install.XXXXXX")
mount_point="$work_directory/mount"
dmg_path="$work_directory/auq-wizard.dmg"
checksum_path="$work_directory/auq-wizard.dmg.sha256"
staged_app="$work_directory/$APP_NAME"
previous_app="$work_directory/previous.app"
mounted=0

cleanup() {
  if [ "$mounted" -eq 1 ]; then
    hdiutil detach "$mount_point" -quiet >/dev/null 2>&1 || true
  fi
  if [ ! -e "$DESTINATION" ] && [ -d "$previous_app" ]; then
    mv "$previous_app" "$DESTINATION" >/dev/null 2>&1 || true
  fi
  rm -rf -- "$work_directory"
}
trap cleanup EXIT HUP INT TERM

printf 'Downloading AUQ Wizard…\n'
fetch_file "$dmg_url" "$dmg_path"
fetch_file "$checksum_url" "$checksum_path"
expected_checksum=$(awk 'NR == 1 { print $1 }' "$checksum_path")
actual_checksum=$(shasum -a 256 "$dmg_path" | awk '{ print $1 }')
[ -n "$expected_checksum" ] || fail "the release checksum is empty"
[ "$actual_checksum" = "$expected_checksum" ] || fail "the DMG checksum does not match"

mkdir "$mount_point"
hdiutil attach "$dmg_path" -nobrowse -readonly -mountpoint "$mount_point" -quiet
mounted=1
source_app="$mount_point/$APP_NAME"
[ -d "$source_app" ] || fail "$APP_NAME was not found in the DMG"
[ "$(read_bundle_identifier "$source_app")" = "$BUNDLE_IDENTIFIER" ] || fail "the downloaded app has an unexpected bundle identifier"

ditto "$source_app" "$staged_app"
hdiutil detach "$mount_point" -quiet
mounted=0

if [ -e "$DESTINATION" ]; then
  [ -d "$DESTINATION" ] || fail "$DESTINATION exists and is not an app directory"
  [ "$(read_bundle_identifier "$DESTINATION")" = "$BUNDLE_IDENTIFIER" ] || fail "$DESTINATION belongs to a different app and was not replaced"
  mv "$DESTINATION" "$previous_app"
fi
mv "$staged_app" "$DESTINATION"

printf 'Installed AUQ Wizard at %s\n' "$DESTINATION"
if [ "${AUQ_NO_LAUNCH:-0}" != "1" ]; then
  open "$DESTINATION"
  printf 'Complete setup with “Install integrations” in the app.\n'
fi
