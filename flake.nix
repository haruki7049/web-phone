{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    systems.url = "github:nix-systems/default";
    crane.url = "github:ipetkov/crane";
    flake-compat.url = "github:edolstra/flake-compat";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;

      imports = [
        inputs.treefmt-nix.flakeModule
      ];

      perSystem =
        {
          pkgs,
          lib,
          system,
          ...
        }:
        let
          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rust;
          overlays = [ inputs.rust-overlay.overlays.default ];

          src = lib.cleanSource ./.;
          nativeBuildInputs = [
            # Compiler
            rust
            pkgs.pkg-config # pkg-config

            # LSP
            pkgs.nil

            # Debugging tools
            pkgs.websocat
          ];
          buildInputs = lib.optionals pkgs.stdenv.isLinux [
            pkgs.alsa-lib
          ];

          cargoArtifacts = craneLib.buildDepsOnly {
            inherit src nativeBuildInputs buildInputs;
          };
          app = craneLib.buildPackage {
            inherit
              src
              cargoArtifacts
              nativeBuildInputs
              buildInputs
              ;
            strictDeps = true;

            doCheck = true;
          };
          cargo-clippy = craneLib.cargoClippy {
            inherit
              src
              cargoArtifacts
              nativeBuildInputs
              buildInputs
              ;
            cargoClippyExtraArgs = "--verbose -- --deny warnings";
          };
          cargo-doc = craneLib.cargoDoc {
            inherit
              src
              cargoArtifacts
              nativeBuildInputs
              buildInputs
              ;
          };
        in
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system overlays;
          };

          treefmt = {
            projectRootFile = ".git/config";

            # Nix
            programs.nixfmt.enable = true;

            # Rust
            programs.rustfmt.enable = true;
            settings.formatter.rustfmt.command = "${rust}/bin/rustfmt";

            # TOML
            programs.taplo.enable = true;

            # GitHub Actions
            programs.actionlint.enable = true;

            # Markdown
            programs.mdformat.enable = true;

            # ShellScript
            programs.shellcheck.enable = true;
            programs.shfmt.enable = true;
          };

          packages = {
            inherit app;
            default = app;
            doc = cargo-doc;
          };

          checks = {
            inherit app cargo-clippy cargo-doc;
          };

          devShells.default = pkgs.mkShell {
            inherit nativeBuildInputs buildInputs;

            shellHook = ''
              export PS1="\n[nix-shell:\w]$ "
            '';
          };
        };
    };
}
