{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    rustc
    cargo
    clippy
    pkg-config
    SDL2
    SDL2_gfx
    SDL2_ttf
    SDL2_image
    alsa-lib
    libjack2
  ];

  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    SDL2
    SDL2_gfx
    SDL2_ttf
    SDL2_image
    alsa-lib
    libjack2
  ]);

  shellHook = ''
    echo "Eden DAW development environment ready"
  '';
}
