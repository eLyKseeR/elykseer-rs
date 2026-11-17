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
    sha256 = "0wvh257vj4dgq11v7kl293sx6h4n9m3c7wz0bmxwq2c51sl4hwd9";
  };
  rustOverlay = fetchTarball {
    url = "https://github.com/oxalica/rust-overlay/archive/2ff7d39b901d79ee26d26f9fa32f90511eecaa0e.tar.gz";
    sha256 = "0f3zkavn473bd196w31v314x8cdpp0lp316xh2w237562fdfp3fg";
  };
  rustVersion = "1.99.1";
}
