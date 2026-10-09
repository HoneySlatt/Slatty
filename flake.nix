{
  description = "SlattyLauncher — native GOG launcher";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      # Dummy GalaxyCommunication service from Comet's sources, cross-built for Windows.
      galaxyCommunication =
        pkgs:
        pkgs.pkgsCross.mingwW64.stdenv.mkDerivation {
          pname = "galaxy-communication";
          inherit (pkgs.comet-gog) version src;
          sourceRoot = "source/dummy-service";
          buildPhase = "$CC -O2 -static communication.c -o GalaxyCommunication.exe";
          installPhase = "install -Dm644 GalaxyCommunication.exe $out/share/slatty/GalaxyCommunication.exe";
          meta.license = pkgs.lib.licenses.asl20;
        };
    in
    {
      packages = forAll (pkgs: {
        galaxy-communication = galaxyCommunication pkgs;
      });

      devShells = forAll (
        pkgs:
        let
          runtimeLibs = with pkgs; [
            wayland
            libxkbcommon
            vulkan-loader
            libGL
            libx11
            libxcursor
            libxi
            libxrandr
            dbus
          ];
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              rust-analyzer
              pkg-config
              sqlite
              comet-gog
              umu-launcher
            ];
            buildInputs = runtimeLibs;
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
            SLATTY_GALAXY_COMMUNICATION = "${galaxyCommunication pkgs}/share/slatty/GalaxyCommunication.exe";
            shellHook = ''
              export PATH="$(git rev-parse --show-toplevel 2>/dev/null || pwd)/target/debug:$PATH"
            '';
          };
        }
      );
    };
}
