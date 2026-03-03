{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";

  outputs = { self, nixpkgs }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems (system: f system);
      nixpkgsFor = forAllSystems (system: import nixpkgs {
        inherit system;
      });
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
    };
}
