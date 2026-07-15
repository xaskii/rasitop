{
  description = "rasitop development and full-symbol profiling shell";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  inputs.rust-overlay = {
    url = "github:oxalica/rust-overlay";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    nixpkgs,
    rust-overlay,
    ...
  }: let
    system = "aarch64-darwin";
    pkgs = import nixpkgs {
      inherit system;
      overlays = [(import rust-overlay)];
    };
    rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
    rustPlatform = pkgs.makeRustPlatform {
      cargo = rustToolchain;
      rustc = rustToolchain;
    };
    cargoInstruments = pkgs.callPackage ./nix/cargo-instruments.nix {
      inherit rustPlatform;
    };
    profileApp = pkgs.callPackage ./nix/profile.nix {
      inherit cargoInstruments rustToolchain;
    };
  in {
    packages.${system} = {
      cargo-instruments = cargoInstruments;
      profile = profileApp;
      default = profileApp;
    };

    apps.${system} = {
      profile = {
        type = "app";
        program = "${profileApp}/bin/rasitop-profile";
      };
      default = {
        type = "app";
        program = "${profileApp}/bin/rasitop-profile";
      };
    };

    devShells.${system}.default = pkgs.mkShell {
      packages = [
        cargoInstruments
        rustToolchain
      ];

      shellHook = ''
        # Rust, Swift, and the final link must use the active Xcode SDK rather
        # than an SDK or compiler wrapper injected by Nix.
        unset SDKROOT DEVELOPER_DIR
        unset NIX_CC NIX_CFLAGS_COMPILE NIX_LDFLAGS
        unset LD CC CXX CFLAGS CPPFLAGS LDFLAGS
        export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
      '';
    };
  };
}
