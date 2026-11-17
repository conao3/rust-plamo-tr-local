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
        let
          vulkanInputs = [
            pkgs.vulkan-headers
            pkgs.vulkan-loader
            pkgs.spirv-headers
          ];
          plamoTrLocal =
            {
              pname,
              features ? [ ],
              extraNativeBuildInputs ? [ ],
              extraBuildInputs ? [ ],
            }:
            pkgs.rustPlatform.buildRustPackage {
              inherit pname;
              buildFeatures = features;
              version = "0.1.0";
              src = pkgs.lib.cleanSource ./.;
              cargoLock = {
                lockFile = ./Cargo.lock;
                outputHashes."llama-cpp-2-0.1.162" = "sha256-7jI4+6tJ6s69ojMeqUbDHbbUfmlMbnecpvWrTM7iLEM=";
              };
              nativeBuildInputs = [
                pkgs.cmake
                pkgs.pkg-config
                pkgs.rustPlatform.bindgenHook
              ]
              ++ extraNativeBuildInputs;
              dontUseCmakeConfigure = true;
              buildInputs = [ pkgs.aws-lc ] ++ extraBuildInputs;
              meta = {
                description = "Translate text from the command line with PLaMo 3 Translate running on your machine";
                homepage = "https://github.com/conao3/rust-plamo-tr-local";
                license = pkgs.lib.licenses.asl20;
                mainProgram = "plamo-tr-local";
                platforms = pkgs.lib.platforms.unix;
              };
            };
        in
        {
          packages.default = plamoTrLocal { pname = "plamo-tr-local"; };

          packages.vulkan = plamoTrLocal {
            pname = "plamo-tr-local-vulkan";
            features = [ "vulkan" ];
            extraNativeBuildInputs = [ pkgs.shaderc ];
            extraBuildInputs = vulkanInputs;
          };

          devShells.default = pkgs.mkShell {
            packages =
              with pkgs;
              [
                cargo
                clippy
                cmake
                gnumake
                rust-analyzer
                rustc
                rustfmt
                shaderc
              ]
              ++ vulkanInputs;
            nativeBuildInputs = [ pkgs.rustPlatform.bindgenHook ];
          };

          treefmt.programs = {
            nixfmt.enable = true;
            rustfmt.enable = true;
          };
        };
    };
}
