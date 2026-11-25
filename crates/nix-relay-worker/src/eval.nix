args: let
  inherit (builtins) concatStringsSep filter foldl' head mapAttrs tail;

  root = builtins.fetchTree {
    type = "path";
    path = args.source;
    inherit (args.sourceInfo) narHash;
  };
  rootDir =
    root.outPath
    + (
      if args.subdir == ""
      then ""
      else "/" + args.subdir
    );
  rootSourceInfo = args.sourceInfo // {inherit (root) outPath;};

  lockFile =
    if builtins.pathExists (rootDir + "/flake.lock")
    then builtins.fromJSON (builtins.readFile (rootDir + "/flake.lock"))
    else {
      root = "root";
      nodes.root = {};
    };

  resolveInput = spec:
    if builtins.isList spec
    then getInputByPath lockFile.root spec
    else spec;

  getInputByPath = node: path:
    if path == []
    then node
    else getInputByPath (resolveInput lockFile.nodes.${node}.inputs.${head path}) (tail path);

  nodes =
    mapAttrs (
      key: node: let
        isRoot = key == lockFile.root;
        isRelative = node.locked.type or null == "path" && builtins.substring 0 1 node.locked.path != "/";
        parent = nodes.${getInputByPath lockFile.root node.parent};

        sourceInfo =
          if isRoot
          then rootSourceInfo
          else if isRelative
          then parent.sourceInfo
          else builtins.fetchTree (node.info or {} // removeAttrs node.locked ["dir"]);

        subdir =
          if isRoot
          then args.subdir
          else node.locked.dir or "";

        subdirBase =
          if !isRoot && isRelative
          then
            parent.outPath
            + (
              if node.locked.path == ""
              then ""
              else "/" + node.locked.path
            )
          else sourceInfo.outPath;

        outPath =
          subdirBase
          + (
            if subdir == ""
            then ""
            else "/" + subdir
          );

        flake = import (outPath + "/flake.nix");
        inputs = mapAttrs (_: spec: nodes.${resolveInput spec}.result) (node.inputs or {});
        outputs = flake.outputs (inputs // {self = result;});
        result =
          outputs
          // sourceInfo
          // {
            inherit outPath inputs outputs sourceInfo;
            _type = "flake";
          };
      in {
        result =
          if node.flake or true
          then result
          else sourceInfo // {inherit sourceInfo outPath;};
        inherit outPath sourceInfo;
      }
    )
    lockFile.nodes;

  flake = nodes.${lockFile.root}.result;

  has = path: set:
    path
    == []
    || (builtins.isAttrs set && set ? ${head path} && has (tail path) set.${head path});

  found = filter (path: has path flake) args.attributes;

  show = path: "'${concatStringsSep "." path}'";
in
  if found == []
  then throw "flake does not provide attribute ${concatStringsSep ", " (map show args.attributes)}"
  else foldl' (set: name: set.${name}) flake (head found)
