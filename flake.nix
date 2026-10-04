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
          # only what the build reads: editing tests or docs doesn't rebuild
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./durst
              ./durst-proto
              ./durstctl
              ./tools
              ./contrib
            ];
          };
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
            # pipewire-rs generates its bindings with bindgen
            rustPlatform.bindgenHook
          ];
          buildInputs = runtimeLibs ++ [ pipewire ];

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

        # durst on other compositors, each in a NixOS VM (needs KVM), also
        # with HiDPI scales: nix build .#checks.x86_64-linux.vm-hyprland -L
        checks = lib.optionalAttrs stdenv.hostPlatform.isLinux (
          lib.listToAttrs (
            map
              (
                {
                  compositor,
                  scale ? "1",
                }:
                lib.nameValuePair
                  (
                    "vm-${compositor}"
                    + lib.optionalString (scale != "1") "-scale${lib.replaceStrings [ "." ] [ "_" ] scale}"
                  )
                  (
                    import ./nix/vm-test.nix {
                      inherit
                        pkgs
                        lib
                        compositor
                        scale
                        ;
                      durst = packages.default;
                    }
                  )
              )
              [
                { compositor = "sway"; }
                { compositor = "labwc"; }
                { compositor = "hyprland"; }
                { compositor = "niri"; }
                {
                  compositor = "sway";
                  scale = "2";
                }
                {
                  compositor = "sway";
                  scale = "1.5";
                }
                {
                  compositor = "hyprland";
                  scale = "2";
                }
              ]
          )
        );

        devShell = mkShell rec {
          nativeBuildInputs = [
            pkg-config
            cargo
            rustc
            dbus
            rustfmt
            clippy
            cargo-outdated
            cargo-fuzz
            rustPlatform.bindgenHook

            # scripts/visual (pipewire: a private instance for the volume tests)
            sway
            grim
            foot
            libnotify
            pipewire
            (python3.withPackages (ps: [ ps.pillow ]))
          ];
          buildInputs = runtimeLibs ++ [ pipewire ];
          LD_LIBRARY_PATH = "${lib.makeLibraryPath buildInputs}";
        };
      }
    );
}
