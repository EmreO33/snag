#!/bin/bash
# Every Linux package from one built binary:
#
#   packaging/linux/build-packages.sh <version> <path to snag binary> [outdir]
#
# writes, into outdir (default dist/):
#   Snag-<version>-x86_64.AppImage       runs anywhere, updates itself
#   snag_<version>_amd64.deb             Debian, Ubuntu, Mint, Pop!_OS
#   snag-<version>-1.x86_64.rpm          Fedora, openSUSE
#   snag-<version>-linux-x86_64.tar.gz   the installed tree, which the AUR
#                                        package unpacks
#   snag-linux-x86_64                    the bare binary, which a loose copy
#                                        of Snag updates itself from
#
# The release workflow runs it inside Ubuntu 22.04, so everything here needs
# glibc 2.35 and nothing newer. Run from anywhere; it finds the repo itself.
# nfpm and appimagetool are fetched once into target/packaging-tools unless
# they are already on PATH.
set -euo pipefail

version="${1:?usage: build-packages.sh <version> <binary> [outdir]}"
binary="$(readlink -f "${2:?usage: build-packages.sh <version> <binary> [outdir]}")"
root="$(cd "$(dirname "$0")/../.." && pwd)"
out="$(mkdir -p "${3:-$root/dist}" && cd "${3:-$root/dist}" && pwd)"
id="io.github.EmreO33.Snag"

NFPM_VERSION="2.47.0"
APPIMAGETOOL_VERSION="1.9.1"
tools="$root/target/packaging-tools"
mkdir -p "$tools"
export PATH="$tools:$PATH"

if ! command -v nfpm >/dev/null; then
    curl -fsSL "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/nfpm_${NFPM_VERSION}_Linux_x86_64.tar.gz" \
        | tar -xz -C "$tools" nfpm
fi
# The appimagetool from AppImage/appimagetool, not the retired AppImageKit
# one: it builds with the static runtime, which needs no libfuse2. The old
# runtime did, and Ubuntu stopped installing libfuse2 in 22.04, so a fresh
# Ubuntu, Fedora or Arch could not start the AppImage at all.
if ! command -v appimagetool >/dev/null; then
    curl -fsSL -o "$tools/appimagetool" \
        "https://github.com/AppImage/appimagetool/releases/download/${APPIMAGETOOL_VERSION}/appimagetool-x86_64.AppImage"
    chmod +x "$tools/appimagetool"
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# --- the installed tree, which every package is a wrapping of --------------
tree="$work/tree"
install -Dm755 "$binary" "$tree/usr/bin/snag"
install -Dm644 "$root/packaging/linux/$id.desktop" "$tree/usr/share/applications/$id.desktop"
install -Dm644 "$root/packaging/linux/$id.metainfo.xml" "$tree/usr/share/metainfo/$id.metainfo.xml"
install -Dm644 "$root/assets/icon.png" "$tree/usr/share/icons/hicolor/256x256/apps/$id.png"
install -Dm644 "$root/LICENSE" "$tree/usr/share/licenses/snag/LICENSE"

tar -C "$tree" -czf "$out/snag-$version-linux-x86_64.tar.gz" usr
cp "$binary" "$out/snag-linux-x86_64"

# --- deb and rpm -------------------------------------------------------------
# nfpm reads the binary from dist/linux/snag, relative to the repo root.
mkdir -p "$root/dist/linux"
cp "$binary" "$root/dist/linux/snag"
(
    cd "$root"
    VERSION="$version" nfpm package -f packaging/linux/nfpm.yaml -p deb -t "$out/"
    VERSION="$version" nfpm package -f packaging/linux/nfpm.yaml -p rpm -t "$out/"
)
rm -rf "$root/dist/linux"

# --- AppImage ----------------------------------------------------------------
appdir="$work/Snag.AppDir"
cp -a "$tree" "$appdir"
install -Dm755 "$root/packaging/linux/AppRun" "$appdir/AppRun"
cp "$root/packaging/linux/$id.desktop" "$appdir/$id.desktop"
cp "$root/assets/icon.png" "$appdir/$id.png"
ln -s "$id.png" "$appdir/.DirIcon"
# No FUSE inside a container, so the tool runs from its own extraction.
ARCH=x86_64 appimagetool --appimage-extract-and-run --no-appstream \
    "$appdir" "$out/Snag-$version-x86_64.AppImage"

ls -la "$out"
