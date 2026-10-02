{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      with pkgs;
      let
        # loaded at runtime by the renderer and the Wayland backend
        runtimeLibs = [
          libxkbcommon
          libGL
          vulkan-loader
          wayland
        ];
      in
      rec {
        packages.default = rustPlatform.buildRustPackage {
          pname = "durst";
          version = (lib.importTOML ./durst/Cargo.toml).package.version;
          src = lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;

          # the test tools in tools/ are not installed
          cargoBuildFlags = [
            "-p"
            "durst"
            "-p"
            "durstctl"
          ];
          cargoTestFlags = [
            "-p"
            "durst"
            "-p"
            "durst-proto"
            "-p"
            "durstctl"
          ];

          nativeBuildInputs = [
            pkg-config
            makeWrapper
            installShellFiles
          ];
          buildInputs = runtimeLibs;

          # build.rs writes completions and man pages into */scripts
          postInstall = ''
            for bin in durst durstctl; do
              installShellCompletion --cmd $bin \
                --bash $bin/scripts/completion/$bin.bash \
                --fish $bin/scripts/completion/$bin.fish \
                --zsh $bin/scripts/completion/_$bin
              installManPage $bin/scripts/man/*.1
            done

            install -Dm644 contrib/systemd/durst.service -t $out/lib/systemd/user
            install -Dm644 contrib/dbus/*.service -t $out/share/dbus-1/services
            substituteInPlace $out/lib/systemd/user/durst.service $out/share/dbus-1/services/*.service \
              --replace-fail @bindir@ $out/bin
            install -Dm644 contrib/config.toml -t $out/share/doc/durst
          '';

          postFixup = ''
            wrapProgram $out/bin/durst --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
          '';

          meta = {
            description = "Wayland notification daemon inspired by dunst";
            homepage = "https://github.com/durst-notification/durst";
            mainProgram = "durst";
            platforms = lib.platforms.linux;
          };
        };

        devShell = mkShell rec {
          nativeBuildInputs = [
            pkg-config
            cargo
            rustc
            dbus
            rustfmt
            clippy
            cargo-outdated

            # scripts/visual
            sway
            grim
            foot
            libnotify
            (python3.withPackages (ps: [ ps.pillow ]))
          ];
          buildInputs = runtimeLibs;
          LD_LIBRARY_PATH = "${lib.makeLibraryPath buildInputs}";
        };
      }
    );
}
