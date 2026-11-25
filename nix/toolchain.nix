{inputs, ...}: {
  perSystem = {
    pkgs,
    system,
    ...
  }: {
    _module.args = {
      pkgs = import inputs.nixpkgs {
        inherit system;
        overlays = [(import inputs.rust-overlay)];
      };
      rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ../rust-toolchain.toml;
      withMold = pkgs.stdenvAdapters.useMoldLinker;
    };
  };
}
