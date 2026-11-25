{self, ...}: {
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
        port = lib.mkOption {
          type = lib.types.port;
          default = 7375;
          description = "Port to listen on.";
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
      };

      config = lib.mkIf cfg.enable {
        systemd.services.nix-relay-server = {
          description = "nix-relay server";
          wantedBy = ["multi-user.target"];
          wants = ["network-online.target"];
          after = ["network-online.target"];
          serviceConfig = {
            ExecStart = lib.escapeShellArgs [
              "${cfg.package}/bin/nix-relay-server"
              "--listen"
              "[::]:${toString cfg.port}"
              "--cache-url"
              cfg.cacheUrl
              "--cache-store"
              cfg.cacheStore
            ];
            DynamicUser = true;
            Restart = "always";
            RestartSec = 5;
          };
        };
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
      };

      config = lib.mkIf cfg.enable {
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
              ++ lib.optional cfg.cacheHost "--cache-host");
            Restart = "always";
            RestartSec = 5;
          };
        };
      };
    };
  };
}
