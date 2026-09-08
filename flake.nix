{
  description = "A nix development shell and build environment for shieldd";

  inputs = {
    # nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    crane = {
      url = "github:ipetkov/crane";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane, ... }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          # Select the Shieldd source
          shielddRelease = null; # Use the local working copy
          # Build grpcui from source, for Reflection v1 support.
          # https://github.com/fullstorydev/grpcui/issues/322
          # To update the grpcui hash values, run:
          # nix-prefetch-git --url https://github.com/fullstorydev/grpcui --rev 483f037ec98b89200353c696d990324318f8df98
          grpcUiRelease = {
            version = "1.4.2-pre.1";
            sha256 = "sha256-3vjJNa1bXoMGZXPRyVqhxYZPX5FDp8Efy+w6gdx0pXE=";
            vendorHash = "sha256-j7ZJeO9vhjOoR8aOOJymDM6D7mPAJQoD4O6AyAsErRY=";
            rev = "483f037ec98b89200353c696d990324318f8df98";
          };

          # Set up for Rust builds, pinned to the Rust toolchain version in the Shieldd repository
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };
          # Add nightly Rust toolchain, required for building the rustdocs with combined index landing page.
          # https://github.com/oxalica/rust-overlay/blob/master/README.md#cheat-sheet-common-usage-of-rust-bin
          nightlyRustToolchain = pkgs.rust-bin.selectLatestNightlyWith(toolchain: toolchain.default);
          nightlyCraneLib = (crane.mkLib pkgs).overrideToolchain nightlyRustToolchain;

          # Important environment variables so that the build can find the necessary libraries
          PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig";
          LIBCLANG_PATH="${pkgs.libclang.lib}/lib";
          ROCKSDB_LIB_DIR="${pkgs.rocksdb.out}/lib";
        in with pkgs; with pkgs.lib; let
          # Common development packages for all shells
          commonDevPackages = [
            openssl
            buf
            cargo-hack
            cargo-nextest
            cargo-release
            cargo-watch
            dbus
            go
            grpcui
            grpcurl
            just
            libusb1
            nix-prefetch-scripts
            protobuf
            llvmPackages.lld
            rocksdb
            rsync
            sqlfluff
            toml-cli
          ];

          # Keep Rust-only CI independent from standalone node and observability
          # tooling. In particular, evaluating this shell must not build CometBFT.
          rustCiPackages = [
            openssl
            cargo-hack
            cargo-nextest
            cargo-release
            dbus
            go
            just
            libusb1
            llvmPackages.lld
            protobuf
            python3
            ripgrep
            toml-cli
          ];

          # Native and system dependencies needed to build the Rust workspace in dev shells.
          shellBuildInputs = if stdenv.hostPlatform.isDarwin then
            with pkgs.darwin.apple_sdk.frameworks; [
              clang
              pkg-config
              sqlite
              SystemConfiguration
              CoreServices
            ]
          else
            [
              clang
              pkg-config
              sqlite
            ];

          # Common shell hook content
          commonShellHook = ''
            export LIBCLANG_PATH=${LIBCLANG_PATH}
            export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc} # Required for rust-analyzer
            export ROCKSDB_LIB_DIR=${ROCKSDB_LIB_DIR}
            export LD_LIBRARY_PATH=${lib.makeLibraryPath [
              pkgs.bzip2
              pkgs.dbus
              pkgs.lz4
              pkgs.openssl
              pkgs.rocksdb
              pkgs.snappy
              pkgs.sqlite
              pkgs.stdenv.cc.cc
              pkgs.zstd
            ]}:''${LD_LIBRARY_PATH:-}
            export RUST_LOG="info,network_integration=debug,pcli=info,shieldd=info"
          '';

          # grpcui
          grpcui = (buildGoModule rec {
            pname = "grpcui";
            version = grpcUiRelease.version;
            subPackages = [ "cmd/grpcui" ];
            src = fetchFromGitHub {
              owner = "fullstorydev";
              repo = "grpcui";
              rev = "${grpcUiRelease.rev}";
              hash = grpcUiRelease.sha256;
            };
            vendorHash = grpcUiRelease.vendorHash;
            meta = {
              description = "An interactive web UI for gRPC, along the lines of postman";
              homepage = "https://github.com/fullstorydev/grpcui";
              license = licenses.mit;
            };
          }).overrideAttrs (_: { doCheck = false; }); # Disable tests to improve build times
        in rec {
          packages = {
            shieldd =
              let
                rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
                craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
              in
              (craneLib.buildPackage {
                pname = "shieldd";
                src = cleanSourceWith {
                  src = if shielddRelease == null then craneLib.path ./. else fetchFromGitHub {
                    owner = "mizufinance";
                    repo = "shieldd";
                    rev = "v${shielddRelease.version}";
                    sha256 = "${shielddRelease.sha256}";
                  };
                  filter = path: type:
                    # Retain non-rust asset files as build inputs:
                    # * no_lfs, param, bin: proving and verification parameters
                    # * zip: frontend bundled assets
                    # * sql: database schema files for indexing
                    # * csv: default genesis allocations for testnet generation
                    # * json: default validator info for testnet generation
                    (builtins.match ".*\.(no_lfs|param|bin|zip|sql|csv|json)$" path != null) ||
                    # ... as well as all the normal cargo source files:
                    (craneLib.filterCargoSources path type);
                };
                nativeBuildInputs = [ pkg-config ];
                buildInputs = if stdenv.hostPlatform.isDarwin then
                  with pkgs.darwin.apple_sdk.frameworks; [ clang openssl rocksdb sqlite SystemConfiguration CoreServices ]
                else
                  [ clang dbus openssl rocksdb sqlite ];

                inherit system PKG_CONFIG_PATH LIBCLANG_PATH ROCKSDB_LIB_DIR;
                cargoExtraArgs = "-p shieldd -p pcli";
                meta = {
                  description = "Host-integrated private execution and offline wallet tooling";
                  license = [ licenses.mit licenses.asl20 ];
                };
              }).overrideAttrs (_: { doCheck = false; });
          };
          apps = {
            shieldd.type = "app";
            shieldd.program = "${packages.shieldd}/bin/shieldd";
            pcli.type = "app";
            pcli.program = "${packages.shieldd}/bin/pcli";
          };
          defaultPackage = symlinkJoin {
            name = "shieldd";
            paths = [ packages.shieldd ];
          };
          devShells = {
            ci =
              let
                rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
                craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
              in
              craneLib.devShell {
                inherit LIBCLANG_PATH ROCKSDB_LIB_DIR;
                packages = rustCiPackages ++ shellBuildInputs;
                shellHook = commonShellHook;
              };

            default =
              let
                rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
                craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
              in
              craneLib.devShell {
                inherit LIBCLANG_PATH ROCKSDB_LIB_DIR;
                packages = commonDevPackages ++ shellBuildInputs;
                shellHook = ''
                  ${commonShellHook}
                  echo "Using stable Rust from rust-toolchain.toml"
                '';
              };

            nightly = nightlyCraneLib.devShell {
              inherit LIBCLANG_PATH ROCKSDB_LIB_DIR;
              packages = commonDevPackages ++ shellBuildInputs;
              shellHook = ''
                ${commonShellHook}
                echo "Using nightly Rust toolchain"
              '';
            };
          };
        }
      );
}
