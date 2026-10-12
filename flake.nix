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
      # Loaded by the interface at run time (window, input, graphics, the file chooser).
      runtimeLibs =
        pkgs: with pkgs; [
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
      slattyLauncher =
        pkgs:
        let
          # Programs the launcher starts, found in PATH: umu and Proton run Windows games,
          # bubblewrap holds isolated ones, Comet reports achievements, steam-run-free gives native
          # games the usual Linux layout NixOS lacks, and Real-ESRGAN enhances images once Enhance
          # images with AI is on.
          tools = with pkgs; [
            umu-launcher
            bubblewrap
            comet-gog
            steam-run-free
            realesrgan-ncnn-vulkan
          ];
        in
        pkgs.rustPlatform.buildRustPackage {
          pname = "slatty-launcher";
          version = (pkgs.lib.importTOML ./Cargo.toml).workspace.package.version;
          src = pkgs.lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.makeWrapper ];
          # The tests start real processes and read /proc, which the build sandbox narrows; they
          # run in the development shell.
          doCheck = false;
          postInstall = ''
            install -Dm644 packaging/io.github.HoneySlatt.SlattyLauncher.desktop -t $out/share/applications
            install -Dm644 packaging/io.github.HoneySlatt.SlattyLauncher.metainfo.xml -t $out/share/metainfo
          '';
          # The libraries go into the interface's own search path rather than LD_LIBRARY_PATH,
          # which every game started from it would inherit.
          postFixup = ''
            patchelf --add-rpath ${pkgs.lib.makeLibraryPath (runtimeLibs pkgs)} $out/bin/slatty-gui
            for program in slatty slatty-gui; do
              wrapProgram $out/bin/$program \
                --prefix PATH : ${pkgs.lib.makeBinPath tools} \
                --set-default SLATTY_GALAXY_COMMUNICATION ${galaxyCommunication pkgs}/share/slatty/GalaxyCommunication.exe
            done
          '';
          meta = {
            description = "Native GOG launcher for Linux";
            homepage = "https://github.com/HoneySlatt/SlattyLauncher";
            license = pkgs.lib.licenses.gpl3Plus;
            mainProgram = "slatty-gui";
            platforms = systems;
          };
        };
    in
    {
      packages = forAll (pkgs: {
        default = slattyLauncher pkgs;
        slatty-launcher = slattyLauncher pkgs;
        galaxy-communication = galaxyCommunication pkgs;
      });

      devShells = forAll (pkgs: {
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
            # Isolated games run inside it, with umu's runtime and Proton read-only.
            bubblewrap
            # Enhance images with AI.
            realesrgan-ncnn-vulkan
          ];
          buildInputs = runtimeLibs pkgs;
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtimeLibs pkgs);
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          SLATTY_GALAXY_COMMUNICATION = "${galaxyCommunication pkgs}/share/slatty/GalaxyCommunication.exe";
          shellHook = ''
            export PATH="$(git rev-parse --show-toplevel 2>/dev/null || pwd)/target/debug:$PATH"
          '';
        };
      });
    };
}
