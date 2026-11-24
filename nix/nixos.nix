{self, ...}: let
  tokenArgs = lib: cfg:
    lib.optionals (cfg.tokenFile != null) ["--token-file" "%d/token"];
  tokenCredential = lib: cfg:
    lib.optional (cfg.tokenFile != null) "token:${cfg.tokenFile}";
  tokenOption = lib:
    lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "File holding the token that the relay, workers and nixr share.";
    };
in {
  flake.nixosModules = {
    server = {
      config,
      lib,
      pkgs,
      ...
    }: let
      cfg = config.services.nix-relay.server;
    in {
      options.services.nix-relay.server = {
        enable = lib.mkEnableOption "the nix-relay server";
        package = lib.mkOption {
          type = lib.types.package;
          default = self.packages.${pkgs.stdenv.hostPlatform.system}.nix-relay-server;
          defaultText = lib.literalExpression "nix-relay.packages.\${system}.nix-relay-server";
          description = "The nix-relay-server package to run.";
        };
        address = lib.mkOption {
          type = lib.types.str;
          default = "[::]";
          description = "Address to listen on. Wrap IPv6 addresses in brackets.";
        };
        port = lib.mkOption {
          type = lib.types.port;
          default = 7375;
          description = "Port to listen on.";
        };
        openFirewall = lib.mkOption {
          type = lib.types.bool;
          default = false;
          description = "Whether to open the port in the firewall.";
        };
        cacheUrl = lib.mkOption {
          type = lib.types.str;
          example = "http://mayohiga:5000";
          description = "HTTP binary cache that serves the cache host's store. Workers and nixr fetch built paths from it.";
        };
        cacheStore = lib.mkOption {
          type = lib.types.str;
          example = "ssh-ng://mayohiga";
          description = "Store that workers other than the cache host push outputs and derivations to. Workers push unsigned paths, so the user they connect as must be in the cache host's trusted-users.";
        };
        tokenFile = tokenOption lib;
      };

      config = lib.mkIf cfg.enable {
        systemd.services.nix-relay-server = {
          description = "nix-relay server";
          wantedBy = ["multi-user.target"];
          wants = ["network-online.target"];
          after = ["network-online.target"];
          serviceConfig = {
            ExecStart = lib.escapeShellArgs ([
                "${cfg.package}/bin/nix-relay-server"
                "--listen"
                "${cfg.address}:${toString cfg.port}"
                "--cache-url"
                cfg.cacheUrl
                "--cache-store"
                cfg.cacheStore
              ]
              ++ tokenArgs lib cfg);
            LoadCredential = tokenCredential lib cfg;
            DynamicUser = true;
            Restart = "always";
            RestartSec = 5;
          };
        };
        networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [cfg.port];
      };
    };

    worker = {
      config,
      lib,
      pkgs,
      ...
    }: let
      cfg = config.services.nix-relay.worker;
    in {
      options.services.nix-relay.worker = {
        enable = lib.mkEnableOption "a nix-relay worker";
        package = lib.mkOption {
          type = lib.types.package;
          default = self.packages.${pkgs.stdenv.hostPlatform.system}.nix-relay-worker;
          defaultText = lib.literalExpression "nix-relay.packages.\${system}.nix-relay-worker";
          description = "The nix-relay-worker package to run.";
        };
        relay = lib.mkOption {
          type = lib.types.str;
          example = "ws://mayohiga:7375";
          description = "WebSocket URL of the relay.";
        };
        cacheHost = lib.mkOption {
          type = lib.types.bool;
          default = false;
          description = "Whether the relay's cache serves this machine's store. The cache host evaluates builds and skips copying paths to and from the cache.";
        };
        jobs = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.positive;
          default = null;
          description = "Derivations to build at once. Defaults to Nix's max-jobs.";
        };
        name = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = "Name the relay and nixr show for this worker. Defaults to the hostname.";
        };
        tokenFile = tokenOption lib;
      };

      config = lib.mkIf cfg.enable {
        nix.settings.narinfo-cache-negative-ttl = 0;
        systemd.services.nix-relay-worker = {
          description = "nix-relay worker";
          wantedBy = ["multi-user.target"];
          wants = ["network-online.target"];
          after = ["network-online.target"];
          path = [config.nix.package pkgs.openssh];
          serviceConfig = {
            ExecStart = lib.escapeShellArgs ([
                "${cfg.package}/bin/nix-relay-worker"
                "--relay"
                cfg.relay
              ]
              ++ lib.optional cfg.cacheHost "--cache-host"
              ++ lib.optionals (cfg.jobs != null) ["--jobs" (toString cfg.jobs)]
              ++ lib.optionals (cfg.name != null) ["--name" cfg.name]
              ++ tokenArgs lib cfg);
            LoadCredential = tokenCredential lib cfg;
            Restart = "always";
            RestartSec = 5;
          };
        };
      };
    };
  };
}
