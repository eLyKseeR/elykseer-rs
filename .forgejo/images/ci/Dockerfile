# Shared build image for the CI workflows: the pinned Rust toolchain (ci-env.nix,
# pins.nix) with llvm-tools, cargo-llvm-cov and cargo-deny, so the jobs do not evaluate nix-shell or
# install packages on every run.
#
# - node is needed because the workflows use actions/checkout.
# - docker CLI and buildx are needed because image.yaml builds this image
#   from within the image itself.
#
# Build/push with ci/build-image.sh or the image.yaml workflow.
ARG BASE_IMAGE=code.sbclab.net/elykseer/nixos:1.0.2
FROM ${BASE_IMAGE}

LABEL org.opencontainers.image.title="elykseer-rs CI" \
      org.opencontainers.image.description="Rust toolchain, cargo-llvm-cov, cargo-deny, node and docker CLI for building and testing elykseer-rs" \
      org.opencontainers.image.licenses="GPL-3.0-or-later"

COPY ci-env.nix pins.nix /tmp/ci/
RUN nix-env -if /tmp/ci/ci-env.nix \
 && rm -r /tmp/ci \
 && nix-collect-garbage -d

# the nix docker CLI does not look into the profile for plugins
RUN mkdir -p /root/.docker/cli-plugins \
 && ln -s /root/.nix-profile/libexec/docker/cli-plugins/docker-buildx /root/.docker/cli-plugins/docker-buildx
