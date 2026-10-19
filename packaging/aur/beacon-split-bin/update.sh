#!/usr/bin/env bash
# Points the PKGBUILD at a published release: its version, the checksums of
# what that release actually serves, and the .SRCINFO the AUR reads.
#
#   packaging/aur/beacon-split-bin/update.sh 0.9.2
#
# Run after the release is published, not while it is a draft: the checksums
# are of the files GitHub serves, and a draft serves nothing.
set -euo pipefail

version=${1:?usage: update.sh <version, without the v>}
cd "$(dirname "$0")"

sed -i -e "s/^pkgver=.*/pkgver=${version}/" -e 's/^pkgrel=.*/pkgrel=1/' PKGBUILD
updpkgsums
makepkg --printsrcinfo > .SRCINFO

echo "PKGBUILD and .SRCINFO now describe ${version}. Try it with: makepkg -si"
