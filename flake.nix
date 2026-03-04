{
  inputs = {
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
  };

  outputs = { self, naersk, nixpkgs }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];

      forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems (system: f system);

      nixpkgsFor = forAllSystems (system: import nixpkgs {
        inherit system;
      });

      derivation = pkgs:
        let naersk' = pkgs.callPackage naersk { };
        in
        naersk'.buildPackage {
          src = ./.;
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.dbus ];
          meta.mainProgram = "mqttooth";
        };
    in
    {
      devShell = forAllSystems (system:
        let pkgs = nixpkgsFor.${system};
        in pkgs.mkShell {
          packages = [
            pkgs.cargo
            pkgs.cargo-edit
            pkgs.dbus
            pkgs.mosquitto
            pkgs.pkg-config
            pkgs.rust-analyzer
            pkgs.rustc
            pkgs.rustfmt
          ];
        });

      packages = forAllSystems (system:
        let pkgs = nixpkgsFor.${system};
        in {
          default = derivation pkgs;
        }
      );

      overlay = final: prev: {
        mqttooth = derivation final;
      };
    };
}
