{inputs, ...}: {
  perSystem = {
    system,
    pkgs,
    rustToolchain,
    withMold,
    ...
  }: {
    devShells.default =
      (pkgs.mkShell.override {
        stdenv = withMold pkgs.clangStdenv;
      }) {
        buildInputs = with pkgs; [
          cargo-edit
          cargo-msrv
          cargo-watch
          cargo-deny
          cargo-nextest
          cargo-autoinherit
          inputs.crate2nix.packages.${system}.default
          rustToolchain
          rust-analyzer

          just
          pkg-config
        ];
      };
  };
}
