{
  description = "Translate text from the command line with PLaMo 3 Translate running on your machine";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    treefmt-nix.url = "github:numtide/treefmt-nix";
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-darwin"
      ];

      imports = [ inputs.treefmt-nix.flakeModule ];

      perSystem =
        { pkgs, ... }:
        {
          packages.default = pkgs.rustPlatform.buildRustPackage {
            pname = "plamo-tr-local";
            version = "0.1.0";
            src = pkgs.lib.cleanSource ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
              outputHashes."llama-cpp-2-0.1.162" = "sha256-7jI4+6tJ6s69ojMeqUbDHbbUfmlMbnecpvWrTM7iLEM=";
            };
            nativeBuildInputs = [
              pkgs.cmake
              pkgs.rustPlatform.bindgenHook
            ];
            dontUseCmakeConfigure = true;
            meta = {
              description = "Translate text from the command line with PLaMo 3 Translate running on your machine";
              homepage = "https://github.com/conao3/rust-plamo-tr-local";
              license = pkgs.lib.licenses.asl20;
              mainProgram = "plamo-tr-local";
              platforms = pkgs.lib.platforms.unix;
            };
          };

          devShells.default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              clippy
              cmake
              gnumake
              rust-analyzer
              rustc
              rustfmt
            ];
            nativeBuildInputs = [ pkgs.rustPlatform.bindgenHook ];
          };

          treefmt.programs = {
            nixfmt.enable = true;
            rustfmt.enable = true;
          };
        };
    };
}
