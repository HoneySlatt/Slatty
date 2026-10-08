{
  description = "SlattyLauncher — native GOG launcher";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
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
            shellHook = ''
              export PATH="$(git rev-parse --show-toplevel 2>/dev/null || pwd)/target/debug:$PATH"
            '';
          };
        }
      );
    };
}
