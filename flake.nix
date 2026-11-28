{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      imports = [
        inputs.treefmt-nix.flakeModule
      ];

      perSystem =
        {
          config,
          lib,
          pkgs,
          ...
        }:
        let
          # The replacement of pkgs.haskellPackages
          hp = pkgs.haskell.packages.ghc9103.extend (
            pkgs.haskell.lib.compose.packageSourceOverrides {
              wp-ice = ./packages/wp-ice;
            }
          );

          GHC = hp.ghc;
          nativeBuildInputs = [
            GHC # Haskell compiler
            pkgs.cabal-install # Haskell cabal package manager
            pkgs.haskell-language-server # Haskell language server
            hp.hspec-discover # Hspec test discover & runner tool
            pkgs.nil # Nix LSP
          ];

          buildInputs = [ ];
        in
        {
          treefmt = {
            projectRootFile = ".git/config";

            # Nix
            programs.nixfmt.enable = true;

            # Haskell
            programs.ormolu.enable = true;
            programs.cabal-fmt.enable = true;

            # GitHub Actions
            programs.actionlint.enable = true;

            # Markdown
            programs.mdformat.enable = true;
            # Keep the YAML front matter of agent skills (SKILL.md) intact
            programs.mdformat.plugins = ps: [ ps.mdformat-frontmatter ];

            # ShellScript
            programs.shellcheck.enable = true;
            programs.shfmt.enable = true;
          };

          checks = {
            inherit (hp) wp-ice;
          };

          # packages = {
          #   default = hp.web-phone;
          #   inherit (hp) web-phone;
          # };

          devShells.default = pkgs.mkShell {
            inherit buildInputs nativeBuildInputs;
            inputsFrom = [ config.treefmt.build.devShell ];
          };
        };
    };
}
