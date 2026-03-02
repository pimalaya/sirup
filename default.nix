{
  nixpkgs ? <nixpkgs>,
  pimalaya ? import (fetchTarball "https://github.com/pimalaya/nix/archive/master.tar.gz"),
  ...
}@args:

pimalaya.mkDefault (
  {
    src = ./.;
    version = "0.1.0";
  }
  // removeAttrs args [ "pimalaya" ]
)
