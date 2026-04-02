{
  description = "A Bot to Bridge Discord Events with Calendars";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
    in
    {
      devShells."${system}".default =
        let
          pkgs = import nixpkgs { inherit system; };
        in
        pkgs.mkShell {
          packages = with pkgs; [
            # General development tools
            git

            # Rust App Build Dependencies
            gcc
            pkg-config
            openssl

            # Rust Dev Tools
            cargo
            rustc
          ];

          shellHook = ''
            echo 'Ready...';
          '';
        };
    };
}
