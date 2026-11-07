#!/usr/bin/env bash
# Build a release binary and pack it as dist/uartist-<version>-<arch>-linux.tar.gz.
# The tarball uses a normal install layout (usr/bin, usr/share/...), so it can be
# unpacked into / or /usr/local with --strip-components, or used by packagers.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

version="$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="uartist"))')"
arch="$(uname -m)"
name="uartist-${version}-${arch}-linux"
stage="dist/${name}"

cargo build --release --locked -p uartist

rm -rf "$stage"
install -Dm755 target/release/uartist "$stage/usr/bin/uartist"
install -Dm644 packaging/linux/uartist.desktop "$stage/usr/share/applications/uartist.desktop"
for png in assets/icons/hicolor/*/apps/uartist.png; do
  install -Dm644 "$png" "$stage/usr/share/${png#assets/}"
done
install -Dm644 assets/uartist.svg "$stage/usr/share/icons/hicolor/scalable/apps/uartist.svg"
install -Dm644 LICENSE "$stage/usr/share/licenses/uartist/LICENSE"
install -Dm644 README.md "$stage/usr/share/doc/uartist/README.md"

tar -C dist -czf "dist/${name}.tar.gz" "$name"
(cd dist && sha256sum "${name}.tar.gz" > "${name}.tar.gz.sha256")
echo "dist/${name}.tar.gz"
