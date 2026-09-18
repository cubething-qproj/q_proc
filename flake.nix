# ------------------------------------------
# SPDX-License-Identifier: MIT OR Apache-2.0
# -------------------------------- 𝒒𝒑𝒓𝒐𝒋 --
{
  description = "qproj - Bevy game utilities monorepo";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # bevy_cli ships its own flake exposing a `bevy` package whose
    # binary includes the `lint` subcommand (which in turn dispatches to
    # bevy_lint_driver). Consuming it directly removes the need to vendor
    # the bevy_cli source and rebuild bevy_lint from scratch via crane.
    bevy_cli.url = "github:TheBevyFlock/bevy_cli";
  };

  # GPU driver wrapping (formerly via nixGL inputs + a dedicated `nvidia`
  # devshell) is now handled ad-hoc in the `play` justfile recipe via
  # `nix run github:nix-community/nixGL#nixVulkan<Vendor>`. That keeps the
  # devshell pure (no `--impure`) and avoids paying nixGL's closure cost on
  # every shell entry -- you only pay it when you actually launch a binary.

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    rust-overlay,
    bevy_cli,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {inherit system overlays;};

        # Keep in sync with bevy_cli's rust-toolchain.toml. The
        # upstream flake's `bevy_lint_driver` is built against this exact
        # nightly; running it under any other rustc ABI fails to load.
        mkRustToolchain = {
          extensions,
          targets,
        }:
          pkgs.rust-bin.nightly."2026-04-16".default.override {
            inherit extensions targets;
          };

        rustToolchain = mkRustToolchain {
          extensions = [
            "rustc-codegen-cranelift-preview"
            "rustc-dev"
            "llvm-tools-preview"
            "clippy"
            "rust-analyzer"
            "rust-src"
          ];
          targets = [
            "x86_64-pc-windows-msvc"
            "x86_64-unknown-linux-gnu"
          ];
        };

        # Trimmed toolchain for CI. Drops IDE-only extensions
        # (rust-analyzer, rust-src), the rustc-dev internals extension
        # (only needed to compile rustc plugins ourselves; bevy_lint_driver
        # ships prebuilt), and the windows cross-compile target.
        # cranelift stays: .cargo/config.toml pins it as the dev codegen-backend.
        rustToolchainCi = mkRustToolchain {
          extensions = [
            "rustc-codegen-cranelift-preview"
            "llvm-tools-preview"
            "clippy"
          ];
          targets = [
            "x86_64-unknown-linux-gnu"
          ];
        };

        # The `bevy` CLI from the upstream flake. `bevy lint` is the
        # entry point; the package bundles the lint driver alongside it.
        bevy-cli = bevy_cli.packages.${system}.default;

        linuxDeps = pkgs.lib.optionals pkgs.stdenv.isLinux (with pkgs; [
          alsa-lib
          udev
          wayland
          libxkbcommon
          vulkan-loader
        ]);

        mkShell = {
          toolchain ? rustToolchain,
          extraPackages ? [],
          extraShellHook ? "",
        }:
          pkgs.mkShell {
            nativeBuildInputs = [pkgs.pkg-config];

            buildInputs =
              linuxDeps
              ++ [
                toolchain
                bevy-cli
              ];

            packages = extraPackages;

            shellHook = ''
              export CARGO_TERM_COLOR="always"
              export PYTHONUNBUFFERED=1

              if [ -n "$SSH_CLIENT" ]; then
                export FEATURES=""
              else
                export FEATURES="dylib"
              fi

              if [ -f ".env.local" ]; then
                source ".env.local"
              fi

              ${extraShellHook}
            '';

            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath linuxDeps;
          };

        # Packages used by `qproj-scripts build|check|test`.
        ciPackages = with pkgs; [
          sccache
          mold
          clang
          cargo-nextest
          uv
        ];

        # Full developer toolbox.
        devPackages = with pkgs;
          ciPackages
          ++ [
            patchelf
            cargo-deny
            cargo-llvm-cov
            act
            actionlint
            just
          ];
      in {
        devShells.default = mkShell {extraPackages = devPackages;};
        devShells.ci = mkShell {
          toolchain = rustToolchainCi;
          extraPackages = ciPackages;
          extraShellHook = ''
            export CARGO_PROFILE_DEV_DEBUG=0
          '';
        };
        devShells.ci-coverage = mkShell {
          toolchain = rustToolchainCi;
          extraPackages = ciPackages ++ [pkgs.cargo-llvm-cov];
        };
      }
    );
}
