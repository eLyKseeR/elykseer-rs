# Toolchain of the CI image, installed with `nix-env -if ci-env.nix`.
#
# nixpkgs, rust-overlay and the Rust version are pinned in pins.nix.
let
  pins = import ./pins.nix;
  pkgs = import pins.nixpkgs { overlays = [ (import pins.rustOverlay) ]; };
  # minimal profile: no rust-docs, which would only bloat the image
  rust = pkgs.rust-bin.stable.${pins.rustVersion}.minimal.override {
    extensions = [ "rustfmt" "clippy" "llvm-tools-preview" ];
  };
in
pkgs.buildEnv {
  name = "elykseer-rs-ci";
  paths = with pkgs; [
    # build and test
    rust
    cargo-llvm-cov
    # dependency audit: advisories, licenses, sources (deny.toml)
    cargo-deny
    stdenv.cc
    # actions/checkout and the other actions run on node
    nodejs
    # image.yaml: docker CLI with buildx, and the tools of the daemon discovery
    docker-client
    docker-buildx
    diffutils
    gawk
    gnused
    inetutils
    netcat
  ];
  # the base image profile (bash, coreutils, git, ...) wins on collisions
  meta.priority = 10;
}
