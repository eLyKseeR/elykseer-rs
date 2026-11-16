# Pinned nix inputs, shared by the CI image (ci-env.nix) and the developer
# shell (shell.nix), so both use the same nixpkgs and Rust toolchain.
#
# Update rev and sha256 together, then rebuild the CI image:
#   git ls-remote https://github.com/NixOS/nixpkgs refs/heads/nixos-26.05
#   git ls-remote https://github.com/oxalica/rust-overlay refs/heads/master
#   nix-prefetch-url --unpack https://github.com/<owner>/<repo>/archive/<rev>.tar.gz
# Raise rust-version in Cargo.toml together with rustVersion.
{
  # nixos-26.05
  nixpkgs = fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/f1e5c356f35d07e7b71a6b5e6c9338e0b029e0c0.tar.gz";
    sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
  };
  rustOverlay = fetchTarball {
    url = "https://github.com/oxalica/rust-overlay/archive/2ff7d39b901d79ee26d26f9fa32f90511eecaa0e.tar.gz";
    sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
  };
  rustVersion = "1.99.1";
}
