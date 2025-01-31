{
  description = "Nix Relay - binary cache server and distrobuted builds";

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
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      craneLib = crane.mkLib pkgs;

      clientBuildDeps = with pkgs; [rust-bin.stable.latest.default];
      serverBuildDeps = with pkgs; [elixir mix2nix];
    in {
      devShell = pkgs.mkShell {
        inherit clientBuildDeps serverBuildDeps;
      };
    });
}
