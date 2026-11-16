# nixpkgs, rust-overlay and the Rust version are pinned in
# .forgejo/images/ci/pins.nix, shared with the CI image.
let
  pins = import ./.forgejo/images/ci/pins.nix;
  pkgs = import pins.nixpkgs { overlays = [ (import pins.rustOverlay) ]; };
in

with pkgs;

mkShell {
  buildInputs = [
    (rust-bin.stable.${pins.rustVersion}.default.override {
      targets = [
        "aarch64-unknown-linux-gnu"
        "x86_64-unknown-linux-gnu"
      ];
    })

    #pkgsCross.gnu64.stdenv.cc
    pkgsCross.aarch64-multiplatform.stdenv.cc

    docker
    docker-compose
    curl
    git
    gnused
    gettext
    texinfo
    cmake
    clang
    pkg-config
    cargo-deny

    openssl
    openssl.dev
    zlib
    zstd
  ];

  shellHook = ''
    echo 'nixified environment'
    if [ "$(uname -s)" = "Darwin" ]; then
      export SED=gsed
    else
      export SED=sed
    fi

    export CC=clang
    export CXX=clang++

    export NIX_SSL_CERT_FILE=${cacert}/etc/ssl/certs/ca-bundle.crt
    export CURL_CA_BUNDLE=${cacert}/etc/ssl/certs/ca-bundle.crt
    export SSL_CERT_FILE=${cacert}/etc/ssl/certs/ca-bundle.crt

    #export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-unknown-linux-gnu-gcc
    #export CC_x86_64_unknown_linux_gnu=x86_64-unknown-linux-gnu-gcc
    #export CXX_x86_64_unknown_linux_gnu=x86_64-unknown-linux-gnu-g++

    export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-unknown-linux-gnu-gcc
    export CC_aarch64_unknown_linux_gnu=aarch64-unknown-linux-gnu-gcc
    export CXX_aarch64_unknown_linux_gnu=aarch64-unknown-linux-gnu-g++

    export PKG_CONFIG_ALLOW_CROSS=1
  '';
}
