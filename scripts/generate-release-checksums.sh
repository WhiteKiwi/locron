#!/bin/sh
# Emit bare filenames so already installed exact-name updaters can verify releases.
set -eu
cd "${1:?usage: generate-release-checksums.sh ARTIFACT_DIRECTORY}"
: > SHA256SUMS.txt
for archive in *.tar.gz *.deb *.rpm *.zip; do
    [ -f "$archive" ] || continue
    sha256sum -- "$archive" >> SHA256SUMS.txt
done
test -s SHA256SUMS.txt
