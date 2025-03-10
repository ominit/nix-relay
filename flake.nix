{
  description = "Nix Relay - binary cache server and distributed builds";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs = {
    nixpkgs,
    crane,
    flake-utils,
    rust-overlay,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.default;

      commonRustDeps = with pkgs; [
        rust-bin.stable.latest.default
        pkg-config
        openssl
      ];

      elixirDeps = with pkgs; [
        elixir
      ];

      workerSrc = pkgs.lib.cleanSourceWith {
        src = ./worker;
        filter = path: type:
          craneLib.filterCargoSources path type;
      };

      workerArgs = {
        src = workerSrc;
        buildInputs = commonRustDeps;
        PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
      };

      workerDeps = craneLib.buildDepsOnly workerArgs;

      clientSrc = pkgs.lib.cleanSourceWith {
        src = ./client;
        filter = path: type:
          craneLib.filterCargoSources path type;
      };

      clientArgs = {
        src = clientSrc;
        buildInputs = commonRustDeps;
        PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
      };

      clientDeps = craneLib.buildDepsOnly clientArgs;

      serverSrc = ./server;

      # Function to auto-generate deps.nix from mix.lock
      generateDepsNix = lock:
        pkgs.runCommand "generated-deps.nix" {
          buildInputs = [pkgs.mix2nix];
        } ''
          cp ${lock} mix.lock
          mix2nix > $out
        '';

      # For auto-generating deps.nix right in the build
      autoDepsNix =
        if builtins.pathExists (serverSrc + "/mix.lock")
        then generateDepsNix (serverSrc + "/mix.lock")
        else pkgs.writeText "empty-deps.nix" "{ pkgs }: []";

      mixRelease = pkgs.beam.packages.erlang_28.mixRelease {
        pname = "nix-relay-server";
        version = "0.1.0";
        src = serverSrc;

        mixNixDeps = with pkgs;
          import autoDepsNix {
            inherit lib beamPackages;
          };
      };
    in {
      devShells = {
        default = pkgs.mkShell {
          buildInputs =
            []
            ++ commonRustDeps
            ++ elixirDeps;
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        };

        elixir = pkgs.mkShell {
          buildInputs = elixirDeps;
        };

        rust = pkgs.mkShell {
          buildInputs = commonRustDeps;
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        };
      };

      packages = {
        worker = craneLib.buildPackage (workerArgs
          // {
            cargoArtifacts = workerDeps;
          });

        client = craneLib.buildPackage (clientArgs
          // {
            cargoArtifacts = clientDeps;
          });

        server = mixRelease;
      };

      checks = {
        worker-test = craneLib.cargoTest (workerArgs
          // {
            cargoArtifacts = workerDeps;
          });

        client-test = craneLib.cargoTest (clientArgs
          // {
            cargoArtifacts = clientDeps;
          });

        server-test =
          pkgs.runCommand "nix-relay-server-test" {
            buildInputs = elixirDeps;
          } ''
            cp -r ${serverSrc} server
            chmod -R +w server
            cd server
            export MIX_ENV=test

            # Fix permissions and setup Hex
            mkdir -p ~/.hex
            chmod -R +w ~/.hex

            # Get dependencies and run tests
            mix local.hex --force
            mix local.rebar --force
            mix deps.get
            mix test

            # If tests pass, create a marker file
            mkdir -p $out
            touch $out/passed
          '';
      };
    });
}
