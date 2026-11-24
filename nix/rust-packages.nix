{inputs, ...}: {
  perSystem = {
    lib,
    pkgs,
    rustToolchain,
    withMold,
    ...
  }: let
    src = lib.fileset.toSource {
      root = ../.;
      fileset = lib.fileset.unions [
        ../.cargo
        ../Cargo.lock
        ../Cargo.toml
        ../clippy.toml
        ../Cargo.nix
        ../crates
        ../deny.toml
        ../justfile
      ];
    };
    cargoToml = lib.importTOML ../Cargo.toml;
    cargoConfig = lib.importTOML ../.cargo/config.toml;
    memberNames =
      map
      (memberPath: (lib.importTOML (../. + "/${memberPath}/Cargo.toml")).package.name)
      cargoToml.workspace.members;
    cargoProfiles = import ./cargo-profiles.nix {
      inherit lib memberNames;
      manifest = cargoToml;
      config = cargoConfig;
    };

    mkBuildRustCrate = cratePkgs: let
      buildRustCrate = cratePkgs.buildRustCrate.override {
        stdenv = withMold cratePkgs.stdenv;
        cargo = rustToolchain;
        rustc = rustToolchain;
      };
    in
      crate: let
        procMacro = crate.procMacro or false;
        rustcOpts = args:
          cargoProfiles.rustcOpts ({
              profile =
                if crate.release or true
                then "release"
                else "test";
              inherit (crate) crateName;
            }
            // args);
        buildUnitOpts = rustcOpts {
          buildUnit = true;
          withLto = false;
        };
      in
        buildRustCrate (crate
          // {
            extraRustcOpts =
              (crate.extraRustcOpts or [])
              ++ rustcOpts {
                withLto = !procMacro && lib.all (type: type == "lib" || type == "rlib") (crate.type or ["lib"]);
              };
            extraRustcOptsForBuildRs = (crate.extraRustcOptsForBuildRs or []) ++ buildUnitOpts;
          }
          // lib.optionalAttrs procMacro {
            extraRustcOptsForProcMacro = (crate.extraRustcOptsForProcMacro or crate.extraRustcOpts or []) ++ buildUnitOpts;
          });
    # Cargo.nix calls this once per crate. Rebuilding the mold stdenv and the buildRustCrate
    # override on every call dominated evaluation time.
    hostBuildRustCrate = mkBuildRustCrate pkgs;
    buildRustCrateForPkgs = cratePkgs:
      if cratePkgs.stdenv.hostPlatform.config == pkgs.stdenv.hostPlatform.config
      then hostBuildRustCrate
      else mkBuildRustCrate cratePkgs;
    mkCargoNix = release:
      pkgs.callPackage ../Cargo.nix {
        inherit release buildRustCrateForPkgs;
      };

    unifiedCrates = import ./unified-crates.nix {
      inherit lib;
      inherit (pkgs) stdenv;
    };
    releaseCrates = unifiedCrates {
      cargoNix = mkCargoNix true;
      inherit buildRustCrateForPkgs;
      withDevDependencies = false;
    };
    debugCrates = unifiedCrates {
      cargoNix = mkCargoNix false;
      inherit buildRustCrateForPkgs;
      withDevDependencies = true;
    };

    cargoArtifacts = import ./cargo-artifacts.nix {
      inherit lib;
      inherit (pkgs) formats runCommand;
      root = ../.;
      manifest = cargoToml;
    };

    # dependenciesCommand must resolve the same features and profile as command, or cargo
    # rebuilds the dependencies anyway.
    mkCargoCheck = {
      name,
      command,
      dependenciesCommand ? null,
      nativeBuildInputs ? [],
      buildInputs ? [],
      env ? {},
    }: let
      mkCargoDerivation = {script, ...} @ args:
        (withMold pkgs.stdenv).mkDerivation ((removeAttrs args ["script"])
          // {
            version = "0.0.0";
            strictDeps = true;
            nativeBuildInputs =
              [
                rustToolchain
                pkgs.rustPlatform.cargoSetupHook
                pkgs.pkg-config
              ]
              ++ nativeBuildInputs;
            buildInputs = [pkgs.aws-lc] ++ buildInputs;
            cargoDeps = pkgs.rustPlatform.importCargoLock {
              lockFile = ../Cargo.lock;
            };
            buildPhase = ''
              export HOME="$TMPDIR/home"
              mkdir -p "$HOME"
              export CARGO_TARGET_DIR="$TMPDIR/target"
              export CARGO_TERM_COLOR=always
              export CARGO_BUILD_JOBS="$NIX_BUILD_CORES"
              export NEXTEST_TEST_THREADS="$NIX_BUILD_CORES"

              ${lib.concatLines (lib.mapAttrsToList (name: value: "export ${name}=${lib.escapeShellArg value}") env)}

              runHook preBuild
              ${script}
              runHook postBuild
            '';
          });
      dependencies = mkCargoDerivation {
        pname = "${name}-dependencies";
        src = cargoArtifacts.source;
        script = ''
          ${dependenciesCommand}
          ${cargoArtifacts.removeMembers ''"$CARGO_TARGET_DIR"''}
        '';
        installPhase = ''
          cp -r "$CARGO_TARGET_DIR" "$out"
        '';
        # Stripping or patching would change the artifacts cargo already fingerprinted.
        dontFixup = true;
      };
    in
      mkCargoDerivation {
        pname = name;
        inherit src;
        script =
          lib.optionalString (dependenciesCommand != null) (cargoArtifacts.restore dependencies ''"$CARGO_TARGET_DIR"'')
          + command;
        installPhase = ''
          mkdir -p "$out"
        '';
        passthru = lib.optionalAttrs (dependenciesCommand != null) {inherit dependencies;};
      };

    testChecks = lib.genAttrs' memberNames (name:
      lib.nameValuePair "test-${name}" (debugCrates.withTests name {
        testPreRun = ''
          export SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt
        '';
      }));
  in {
    packages = {
      inherit (releaseCrates.crates) nix-relay-server nix-relay-worker nixr;
      default = releaseCrates.crates.nixr;
    };

    checks =
      testChecks
      // {
        clippy = mkCargoCheck {
          name = "clippy";
          dependenciesCommand = ''
            cargo clippy --workspace --all-targets --all-features
          '';
          command = ''
            ${pkgs.just}/bin/just lint
          '';
        };

        doc = mkCargoCheck {
          name = "doc";
          env.RUSTDOCFLAGS = "--deny warnings";
          dependenciesCommand = ''
            cargo doc --workspace --no-deps
          '';
          command = ''
            cargo doc --workspace --no-deps
          '';
        };

        deny = mkCargoCheck {
          name = "deny";
          nativeBuildInputs = [pkgs.cargo-deny];
          command = ''
            db="$HOME/.cargo/advisory-dbs/advisory-db-3157b0e258782691"
            mkdir -p "$db/.git"
            ln -s ${inputs.advisory-db}/* "$db"
            touch -d @${toString inputs.advisory-db.lastModified} "$db/.git/FETCH_HEAD"
            cargo deny --offline check --allow index-failure
          '';
        };
      };
  };
}
