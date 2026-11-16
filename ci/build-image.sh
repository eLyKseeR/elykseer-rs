#!/bin/sh
# Builds the shared CI image and (with --push) pushes it.
#
#   ci/build-image.sh code.sbclab.net/elykseer/elykseer-rs-ci:latest [--push]
#
# The image name must be the same as the CI_IMAGE repository variable that the
# workflows use. Log in to the registry first (docker login) when pushing.
# Without --push the image is loaded for the local platform only.
# PLATFORMS overrides the platforms pushed, e.g. PLATFORMS=linux/amd64.
set -eu
cd "$(dirname "$0")/.."

image=${1:?usage: ci/build-image.sh IMAGE [--push]}
platforms=${PLATFORMS:-linux/amd64,linux/arm64}

if [ "${2:-}" = "--push" ]; then
    docker buildx build --platform "$platforms" -t "$image" -f .forgejo/images/ci/Dockerfile .forgejo/images/ci --push
else
    docker buildx build -t "$image" -f .forgejo/images/ci/Dockerfile .forgejo/images/ci --load
fi
