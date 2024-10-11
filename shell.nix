let
  rust_overlay = import (builtins.fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz");
  pkgs = import <nixpkgs> { overlays = [ rust_overlay ]; };
  probe-rs-me = (pkgs.callPackage ./derivation.nix {});
in
with pkgs;
mkShell {
  buildInputs =  [
    gdb
    picocom
    udev
    pkg-config
    #probe-rs-tools
    probe-rs-me
    (rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
  ];
}
