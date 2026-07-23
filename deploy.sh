#!/usr/bin/env bash

set -euo pipefail

VERSION=${1:-}
DMG_DIRECTORY="src-tauri/target/release/bundle/dmg"

fail() {
  printf 'deploy: %s\n' "$1" >&2
  exit 1
}

[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "usage: ./deploy.sh <version> (for example: ./deploy.sh 0.2.3)"
[[ "$(git branch --show-current)" == "main" ]] || fail "release from the main branch"
[[ -z "$(git status --porcelain)" ]] || fail "worktree must be clean"

for command in git gh node pnpm shasum; do
  command -v "$command" >/dev/null 2>&1 || fail "$command is required"
done

git ls-remote --exit-code --tags origin "refs/tags/v${VERSION}" >/dev/null 2>&1 && fail "tag v${VERSION} already exists"

node --input-type=module - "$VERSION" <<'NODE'
import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
const replaceVersion = (path, pattern) => {
  const source = readFileSync(path, "utf8");
  const updated = source.replace(pattern, (match, prefix) => `${prefix}${version}"`);

  if (updated === source) {
    throw new Error(`could not update the version in ${path}`);
  }

  writeFileSync(path, updated);
};

replaceVersion("package.json", /("version": ")[^"]+"/);
replaceVersion("src-tauri/Cargo.toml", /(^version = ")[^"]+"/m);
replaceVersion("src-tauri/tauri.conf.json", /("version": ")[^"]+"/);
NODE

pnpm install --frozen-lockfile
pnpm check
pnpm test
pnpm test:rust
pnpm tauri build

DMG="${DMG_DIRECTORY}/auq-wizard_${VERSION}_aarch64.dmg"
CHECKSUM="${DMG}.sha256"
test -f "$DMG" || fail "expected DMG was not created: $DMG"
shasum -a 256 "$DMG" | awk -v name="$(basename "$DMG")" '{ print $1 "  " name }' > "$CHECKSUM"

git add package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
git commit -m "Release v${VERSION}"
git push origin main

git tag "v${VERSION}"
git push origin "v${VERSION}"

gh release create "v${VERSION}" "$DMG" "$CHECKSUM" \
  --verify-tag \
  --latest \
  --title "AUQ Wizard v${VERSION}" \
  --generate-notes
