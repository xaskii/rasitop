{
  fetchCrate,
  lib,
  libiconv,
  openssl,
  pkg-config,
  rustPlatform,
  stdenv,
}:

rustPlatform.buildRustPackage rec {
  pname = "cargo-instruments";
  version = "0.4.17";

  src = fetchCrate {
    inherit pname version;
    hash = "sha256-kM2kRjPGjaq7JJBRvP92yY7NqMAa7/QRmyDXHpMWzjQ=";
  };

  patches = [
    ./patches/cargo-instruments-rust-v0-test.patch
    ./patches/cargo-instruments-non-tty.patch
  ];

  cargoHash = "sha256-AYdvMJJGoO69QB2G8JHPNYhNDFzNdVFna/89UP70jRU=";

  nativeBuildInputs = [pkg-config];
  buildInputs =
    [openssl]
    ++ lib.optionals stdenv.hostPlatform.isDarwin [libiconv];

  meta = {
    description = "Profile Cargo binaries with Xcode Instruments";
    homepage = "https://github.com/cmyr/cargo-instruments";
    license = lib.licenses.mit;
    mainProgram = "cargo-instruments";
    platforms = lib.platforms.darwin;
  };
}
