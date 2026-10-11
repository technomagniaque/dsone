{
  description = "dsone — native Linux client for Deezer and TIDAL (lossless, bit-perfect); fork of SONE";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      packages.${system} = {
        dsone = pkgs.callPackage ./nix/package.nix { src = self; };
        default = self.packages.${system}.dsone;
      };

      apps.${system}.default = {
        type = "app";
        program = "${self.packages.${system}.dsone}/bin/dsone";
      };

      devShells.${system}.default = pkgs.mkShell {
        inputsFrom = [ self.packages.${system}.dsone ];
        packages = [
          pkgs.nodejs
          pkgs.pnpm_11
          pkgs.cargo
          pkgs.rustc
          pkgs.cargo-tauri
        ];
        shellHook = ''
          export GST_PLUGIN_SYSTEM_PATH_1_0="${
            pkgs.lib.makeSearchPath "lib/gstreamer-1.0" [
              pkgs.gst_all_1.gstreamer
              pkgs.gst_all_1.gst-plugins-base
              pkgs.gst_all_1.gst-plugins-good
              pkgs.gst_all_1.gst-plugins-bad
              pkgs.gst_all_1.gst-libav
            ]
          }"
        '';
      };

      formatter.${system} = pkgs.nixfmt;

      checks.${system}.build = self.packages.${system}.dsone;
    };
}
