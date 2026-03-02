# TODO: move this to nixpkgs
# This file aims to be a replacement for the nixpkgs derivation.

{
  buildFeatures ? [ ],
  buildNoDefaultFeatures ? false,
  buildPackages,
  fetchFromGitHub,
  installManPages ? stdenv.buildPlatform.canExecute stdenv.hostPlatform,
  installShellCompletions ? stdenv.buildPlatform.canExecute stdenv.hostPlatform,
  installShellFiles,
  lib,
  openssl,
  pkg-config,
  rustPlatform,
  stdenv,
}:

let
  version = "0.1.0";
  hash = "";
  cargoHash = "";

  inherit (stdenv.hostPlatform)
    isLinux
    isWindows
    isAarch64
    ;

  emulator = stdenv.hostPlatform.emulator buildPackages;
  exe = stdenv.hostPlatform.extensions.executable;

  hasNativeTlsFeature = builtins.elem "native-tls" buildFeatures;

in
rustPlatform.buildRustPackage {
  inherit
    cargoHash
    version
    buildNoDefaultFeatures
    buildFeatures
    ;

  pname = "sirup";

  src = fetchFromGitHub {
    inherit hash;
    owner = "pimalaya";
    repo = "sirup";
    rev = "v${version}";
  };

  env = {
    # OpenSSL should not be provided by vendors, not even on Windows
    OPENSSL_NO_VENDOR = "1";
  };

  nativeBuildInputs =
    [ ]
    ++ lib.optional hasNativeTlsFeature pkg-config
    ++ lib.optional (installManPages || installShellCompletions) installShellFiles;

  buildInputs = lib.optional hasNativeTlsFeature openssl;

  doCheck = false;

  postInstall =
    lib.optionalString (lib.hasInfix "wine" emulator) ''
      export WINEPREFIX="''${WINEPREFIX:-$(mktemp -d)}"
      mkdir -p $WINEPREFIX
    ''
    + ''
      mkdir -p $out/share/{completions,man}
      ${emulator} "$out"/bin/sirup${exe} manuals "$out"/share/man
      ${emulator} "$out"/bin/sirup${exe} completions -d "$out"/share/completions bash elvish fish powershell zsh
    ''
    + lib.optionalString installManPages ''
      installManPage "$out"/share/man/*
    ''
    + lib.optionalString installShellCompletions ''
      installShellCompletion --cmd sirup \
        --bash "$out"/share/completions/sirup.bash \
        --fish "$out"/share/completions/sirup.fish \
        --zsh "$out"/share/completions/_sirup
    '';

  meta = {
    description = "CLI to spawn pre-authenticated IMAP sessions and expose them via Unix sockets";
    mainProgram = "sirup";
    homepage = "https://github.com/pimalaya/sirup";
    changelog = "https://github.com/pimalaya/sirup/blob/v${version}/CHANGELOG.md";
    license = lib.licenses.agpl3Plus;
    maintainers = with lib.maintainers; [ soywod ];
  };
}
