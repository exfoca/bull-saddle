# BullSaddle — Developed by Orion Impact (https://orion-impact.com)
# Licensed under the Apache License, Version 2.0.
# SPDX-License-Identifier: Apache-2.0

{
  description = "BullSaddle — observability and operations for local Git repositories";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      forAllSystems = nixpkgs.lib.genAttrs systems;

      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

      rustToolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

      workspaceManifest = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      projectVersion = workspaceManifest.workspace.package.version;

      bullsPackage =
        system:
        let
          pkgs = pkgsFor system;
          rustToolchain = rustToolchainFor pkgs;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rustToolchain;
            rustc = rustToolchain;
          };
        in
        rustPlatform.buildRustPackage {
          pname = "bulls";
          version = projectVersion;
          src = pkgs.lib.cleanSource ./.;

          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [
            "-p"
            "bulls-cli"
            "--bin"
            "bulls"
          ];
          doCheck = false;

          meta.mainProgram = "bulls";
        };
    in
    {
      packages = forAllSystems (
        system:
        let
          bulls = bullsPackage system;
        in
        {
          inherit bulls;
          default = bulls;
        }
      );

      apps = forAllSystems (
        system:
        let
          bulls = bullsPackage system;
          app = {
            type = "app";
            program = "${bulls}/bin/bulls";
          };
        in
        {
          bulls = app;
          default = app;
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;

          inherit (pkgs) lib;
          inherit (pkgs.stdenv) hostPlatform;

          rustToolchain = rustToolchainFor pkgs;

          developmentTools = with pkgs; [
            rustToolchain

            git
            openssh
            sqlite
            pkg-config
            sccache

            cargo-nextest
            cargo-llvm-cov
            cargo-watch

            cargo-audit
            cargo-auditable
            cargo-cyclonedx
            cargo-dist
            cargo-deny
            cargo-edit
            cargo-outdated
            cargo-machete

            cargo-bloat
            cargo-expand

            hyperfine
            just

            jq
            ripgrep
            fd

            nixd
            nixfmt
            statix
            deadnix
          ];

          linuxTools = lib.optionals hostPlatform.isLinux (
            with pkgs;
            [
              clang
              lld
              mold
              gdb
              strace
              valgrind
            ]
          );

          systemLibraries =
            (with pkgs; [
              sqlite
              openssl
            ])
            ++ lib.optionals hostPlatform.isDarwin (
              with pkgs;
              [
                libiconv
              ]
            );
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = developmentTools ++ linuxTools;
            buildInputs = systemLibraries;

            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
            RUST_BACKTRACE = "1";
            CARGO_NET_GIT_FETCH_WITH_CLI = "true";
            RUSTC_WRAPPER = "${pkgs.sccache}/bin/sccache";

            shellHook = ''
              echo ""
              echo "  ┌─ BULLSADDLE ──────────────────────────────────────────────────┐"
              echo "  │  Observability and operations for local Git repositories      │"
              echo "  │  Rust: rust-toolchain.toml · Dependencies: flake.lock         │"
              echo "  └───────────────────────────────────────────────────────────────┘"
              echo "  ┌─ DEVELOPMENT ─────────────────────────────────────────────────┐"
              echo "  │  just --list             → available project recipes          │"
              echo "  │  just check              → workspace compilation check        │"
              echo "  │  just build              → workspace debug build              │"
              echo "  │  just build-dist         → distribution-profile CLI build     │"
              echo "  └───────────────────────────────────────────────────────────────┘"
              echo "  ┌─ VALIDATION ──────────────────────────────────────────────────┐"
              echo "  │  just quality            → primary development quality gate   │"
              echo "  │  just verify             → full repository verification       │"
              echo "  │  just release-check      → release-specific validation        │"
              echo "  │  just coverage           → LLVM code coverage                 │"
              echo "  │  just security           → dependency and policy checks       │"
              echo "  └───────────────────────────────────────────────────────────────┘"
              echo "  ┌─ LOCALIZATION ────────────────────────────────────────────────┐"
              echo "  │  just i18n-check         → validate translation catalogs      │"
              echo "  │  just i18n-coverage      → show translation coverage          │"
              echo "  │  just i18n-missing LANG  → show missing translations          │"
              echo "  └───────────────────────────────────────────────────────────────┘"
              echo ""
            '';
          };
        }
      );

      formatter = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        pkgs.nixfmt
      );
    };
}
