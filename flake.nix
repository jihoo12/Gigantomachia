{
  description = "Gigantomachia: a small code-first 3D engine";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
    in {
      devShells = nixpkgs.lib.genAttrs systems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          runtimeLibraries = with pkgs; [
            vulkan-loader wayland libxkbcommon
            libX11 libXcursor libXi libXrandr
          ];
        in {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo rustc rustfmt clippy rust-analyzer stdenv.cc
              pkg-config vulkan-tools vulkan-validation-layers
            ];
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
            shellHook = ''
              export LD_LIBRARY_PATH="/run/opengl-driver/lib:${pkgs.lib.makeLibraryPath runtimeLibraries}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
            '';
          };
        });
    };
}
