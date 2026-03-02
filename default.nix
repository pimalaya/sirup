{
  pimalaya ? import (fetchTarball "https://github.com/pimalaya/nix/archive/master.tar.gz"),
  ...
}@args:

pimalaya.mkDefault (
  {
    src = ./.;
    version = "0.1.0";
    mkPackage = (
      {
        lib,
        pkgs,
        rustPlatform,
        buildPackages,
        ...
      }:

      pkgs.callPackage ./package.nix {
        inherit lib rustPlatform buildPackages;
        installShellCompletions = false;
        installManPages = false;
      }
    );
  }
  // removeAttrs args [ "pimalaya" ]
)
