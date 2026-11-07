#!/usr/bin/env bash
# Cross-build the Windows .exe from Linux (needs mingw-w64-gcc and
# `rustup target add x86_64-pc-windows-gnu`) and pack
# dist/uartist-<version>-x86_64-windows-gnu.zip. Official releases are built
# natively on Windows by .github/workflows/release.yml.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

target=x86_64-pc-windows-gnu
version="$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="uartist"))')"
name="uartist-${version}-x86_64-windows-gnu"

cargo build --release --locked -p uartist --target "$target"

rm -rf "dist/$name" "dist/$name.zip"
mkdir -p "dist/$name"
cp "target/$target/release/uartist.exe" LICENSE README.md "dist/$name/"
(cd dist && python3 -c 'import shutil,sys; shutil.make_archive(sys.argv[1], "zip", sys.argv[1])' "$name" && sha256sum "$name.zip" > "$name.zip.sha256")
echo "dist/$name.zip"
