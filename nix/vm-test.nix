{self, ...}: {
  perSystem = {
    pkgs,
    self',
    ...
  }: let
    cachePublicKey = "cache.example.com-1:eIGQXcGQpc00x6/XFcyacLEUmC07u4RAEHt5Y8vdglo=";
    cacheSecretKey = pkgs.writeText "cache-key" "cache.example.com-1:9FhO0w+7HjZrhvmzT1VlAZw4OSAlFGTgC24Seg3tmPl4gZBdwZClzTTHr9cVzJpwsRSYLTu7hEAQe3ljy92CWg==";
    token = pkgs.writeText "nix-relay-token" "test-token";

    common = {lib, ...}: {
      nix.enable = true;
      nix.settings = {
        experimental-features = ["nix-command" "flakes"];
        flake-registry = "";
        substituters = lib.mkForce [];
        trusted-public-keys = lib.mkForce [cachePublicKey];
      };
      networking.firewall.enable = false;
      virtualisation.memorySize = 1024;
    };

    worker = {
      imports = [common self.nixosModules.worker];
      services.nix-relay.worker = {
        enable = true;
        relay = "ws://mayohiga:7375";
        tokenFile = token;
        jobs = 1;
      };
    };
  in {
    checks.vm = pkgs.testers.runNixOSTest {
      name = "nix-relay";
      nodes = {
        mayohiga = {
          imports = [worker self.nixosModules.server];
          services.nix-relay.worker.cacheHost = true;
          services.nix-relay.server = {
            enable = true;
            cacheUrl = "http://mayohiga:5000";
            cacheStore = "ssh-ng://relay@mayohiga";
            tokenFile = token;
          };
          services.harmonia.cache = {
            enable = true;
            signKeyPaths = [cacheSecretKey];
          };
          services.openssh.enable = true;
          users.users.relay.isNormalUser = true;
          nix.settings.trusted-users = ["relay"];
        };

        builder = {
          imports = [worker];
          programs.ssh.extraConfig = ''
            Host mayohiga
              StrictHostKeyChecking no
              UserKnownHostsFile /dev/null
          '';
        };

        client = {
          imports = [common];
          environment.systemPackages = [self'.packages.nixr];
        };
      };

      testScript = ''
        start_all()

        builder.wait_for_unit("multi-user.target")
        builder.succeed("mkdir -p /root/.ssh && ssh-keygen -q -t ed25519 -N ''' -f /root/.ssh/id_ed25519")
        key = builder.succeed("cat /root/.ssh/id_ed25519.pub").strip()
        mayohiga.wait_for_unit("sshd.service")
        mayohiga.succeed(f"install -d -o relay -m 700 /home/relay/.ssh && echo '{key}' > /home/relay/.ssh/authorized_keys && chown relay /home/relay/.ssh/authorized_keys")
        builder.succeed("ssh relay@mayohiga true")

        mayohiga.wait_for_unit("harmonia.socket")
        mayohiga.wait_for_unit("nix-relay-server.service")
        mayohiga.wait_until_succeeds("journalctl -u nix-relay-server | grep -q 'worker joined.*worker=mayohiga'")
        mayohiga.wait_until_succeeds("journalctl -u nix-relay-server | grep -q 'worker joined.*worker=builder'")

        client.wait_for_unit("multi-user.target")
        client.succeed(
            "mkdir -p /root/proj /root/.config/nixr",
            "cp ${./tests/flake.nix} /root/proj/flake.nix",
            "echo first > /root/proj/nonce",
            "printf 'relay = \"ws://mayohiga:7375\"\\ntoken_file = \"${token}\"\\n' > /root/.config/nixr/config.toml",
        )

        with subtest("a build spreads over both workers and lands on the client"):
            out = client.succeed("cd /root/proj && nixr build .#top 2>&1")
            print(out)
            assert "uploading" in out, "the cache host should have asked for the flake source"
            assert "building 6 derivations" in out
            assert "on builder" in out and "on mayohiga" in out, "both workers should have built something"
            assert "building local on mayohiga" in out, "preferLocalBuild derivations belong on the cache host"
            client.succeed("grep -q '^first ' /root/proj/result")

        with subtest("outputs built by the builder are in the cache host's store"):
            top = client.succeed("readlink /root/proj/result").strip()
            mayohiga.succeed(f"nix-store --check-validity {top}")
            mayohiga.succeed(f"nix-store --check-validity $(nix-store -qR {top})")

        with subtest("a second build finds everything cached"):
            out = client.succeed("cd /root/proj && rm result && nixr build .#top 2>&1")
            print(out)
            assert "building" not in out
            client.succeed("test -e /root/proj/result")

        with subtest("a failing derivation fails the build"):
            out = client.fail("cd /root/proj && nixr build .#broken 2>&1")
            print(out)
            assert "about to fail" in out
            assert "broken.drv failed on" in out

        with subtest("nixr is rejected without the token"):
            client.succeed("printf 'relay = \"ws://mayohiga:7375\"\\n' > /root/.config/nixr/config.toml")
            client.fail("cd /root/proj && nixr build .#top")
      '';
    };
  };
}
