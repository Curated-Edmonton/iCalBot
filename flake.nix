{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  # Based on this amazing article:
  # https://log.woodweb.ca/articles/rust-flake/

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" ];
      perSystem = { config, self', pkgs, lib, system, ... }:
        let
          runtimeDeps = with pkgs; [ openssl ];
          buildDeps = with pkgs; [ pkg-config rustPlatform.bindgenHook gcc ];
          devDeps = with pkgs; [ git sqlx-cli sqlite ];

          cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
          msrv = cargoToml.package.rust-version;

          rustPackage = features:
            (pkgs.makeRustPlatform {
              cargo = pkgs.rust-bin.stable.latest.minimal;
              rustc = pkgs.rust-bin.stable.latest.minimal;
            }).buildRustPackage {
              inherit (cargoToml.package) name version;
              src = ./.;
              cargoLock.lockFile = ./Cargo.lock;
              buildFeatures = features;
              buildInputs = runtimeDeps;
              nativeBuildInputs = buildDeps;
              # Uncomment if your cargo tests require networking or otherwise
              # don't play nicely with the Nix build sandbox:
              # doCheck = false;
            };

          mkDevShell = rustc:
            pkgs.mkShell {
              shellHook = ''
                export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc}

                # Load SQLX Completions
                source <(sqlx completions bash)
              '';
              buildInputs = runtimeDeps;
              nativeBuildInputs = buildDeps ++ devDeps ++ [ rustc ];
            };
        in
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ (import inputs.rust-overlay) ];
          };

          packages.default = self'.packages.discordcalendarbot;
          devShells.default = self'.devShells.nightly;

          packages.discordcalendarbot = (rustPackage "");

          devShells.nightly = (mkDevShell (pkgs.rust-bin.selectLatestNightlyWith
            (toolchain: toolchain.default)));
          devShells.stable = (mkDevShell pkgs.rust-bin.stable.latest.default);
          devShells.msrv = (mkDevShell pkgs.rust-bin.stable.${msrv}.default);
          devShells.musl =
            let
              muslCC = pkgs.pkgsCross.musl64.stdenv.cc;
              rustMusl = pkgs.rust-bin.stable.latest.default.override {
                targets = [ "x86_64-unknown-linux-musl" ];
              };
            in
            pkgs.mkShell {
              shellHook = ''
                export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc}

                # Load SQLX Completions
                source <(sqlx completions bash)
              '';
              buildInputs = runtimeDeps;
              nativeBuildInputs = buildDeps ++ devDeps ++ [ rustMusl muslCC ];

              CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${muslCC}/bin/x86_64-unknown-linux-musl-cc";
              CC_x86_64_unknown_linux_musl = "${muslCC}/bin/x86_64-unknown-linux-musl-cc";
              AR_x86_64_unknown_linux_musl = "${muslCC}/bin/x86_64-unknown-linux-musl-ar";
            };
        };
    };
}
