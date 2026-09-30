#!/bin/sh
# Emit bare filenames so already installed exact-name updaters can verify releases.
set -eu
cd "${1:?usage: generate-release-checksums.sh ARTIFACT_DIRECTORY}"
sha256sum -- *.tar.gz *.deb *.rpm > SHA256SUMS.txt
