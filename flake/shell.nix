{
  perSystem = {
    inputs',
    pkgs,
    ...
  }: let
    toolchain = inputs'.fenix.packages.latest;
  in {
    devShells.default = with pkgs;
      mkShell rec {
        nativeBuildInputs = [
          pkg-config
          (toolchain.withComponents [
            "rustc"
            "rust-std"
            "cargo"
            "rust-analyzer"
            "clippy"
            "rust-src"
            "rustfmt"
          ])
        ];
        buildInputs = [openssl];

        LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;
        RUST_LOG = "debug";
      };
  };
}
