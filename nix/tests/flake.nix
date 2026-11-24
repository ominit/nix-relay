{
  outputs = _: let
    system = "x86_64-linux";
    nonce = builtins.replaceStrings ["\n"] [""] (builtins.readFile ./nonce);
    mk = name: deps: extra:
      derivation ({
          inherit name system;
          builder = "/bin/sh";
          args = ["-c" "echo building ${name}; echo ${nonce} ${toString deps} > $out"];
        }
        // extra);
    leaf1 = mk "leaf1" [] {};
    leaf2 = mk "leaf2" [] {};
    leaf3 = mk "leaf3" [] {};
    local = mk "local" [] {preferLocalBuild = true;};
    mid = mk "mid" [leaf1 leaf2] {};
    top = mk "top" [mid leaf3 local] {};
    broken = mk "broken" [leaf1] {args = ["-c" "echo about to fail; exit 3"];};
  in {
    packages.${system} = {
      inherit top broken;
      default = top;
    };
  };
}
