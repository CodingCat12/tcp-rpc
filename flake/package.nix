{
  perSystem = {
    inputs',
    pkgs,
    ...
  }: let
    toolchain = inputs'.fenix.packages.latest;
  in {
    packages.default = with pkgs;
      (makeRustPlatform {
        inherit (toolchain) cargo rustc;
      }).buildRustPackage rec {
        pname = "example";
        version = "0.1.0";

        nativeBuildInputs = [pkg-config];
        buildInputs = [openssl];

        src = ../.;

        cargoLock.lockFile = ../Cargo.lock;
      };
  };
}
