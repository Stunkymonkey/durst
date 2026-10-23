{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    git-hooks.url = "github:cachix/git-hooks.nix";
    git-hooks.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      git-hooks,
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
        # what scripts/visual runs besides durst (pipewire: a private
        # instance for the volume tests)
        visualTestTools = [
          sway
          grim
          foot
          libnotify
          pipewire
          (python3.withPackages (ps: [ ps.pillow ]))
        ];
        # the only fonts durst sees in scripts/visual: the same text sizes on
        # every machine
        visualTestFonts = "${dejavu_fonts}/share/fonts";
        # installed into .git/hooks by the dev shell; clippy stays in CI, it
        # would build the whole workspace on every commit
        preCommit = git-hooks.lib.${system}.run {
          src = ./.;
          hooks = {
            convco.enable = true;
            rustfmt.enable = true;
            nixfmt.enable = true;
            check-merge-conflicts.enable = true;
            check-toml.enable = true;
            check-yaml.enable = true;
            end-of-file-fixer.enable = true;
            trim-trailing-whitespace.enable = true;
          };
        };
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

        # the fakes scripts/visual runs durst against; not installed with it
        packages.test-tools = rustPlatform.buildRustPackage {
          pname = "durst-test-tools";
          inherit (packages.default) version src cargoDeps;
          cargoBuildFlags = [
            "-p"
            "vpointer"
            "-p"
            "fake-logind"
            "-p"
            "fake-player"
            "-p"
            "fake-window"
          ];
          doCheck = false;
          nativeBuildInputs = [ pkg-config ];
          buildInputs = runtimeLibs;
        };

        # durst on other compositors, each in a NixOS VM (needs KVM), also
        # with HiDPI scales: nix build .#checks.x86_64-linux.vm-hyprland -L
        checks = lib.optionalAttrs stdenv.hostPlatform.isLinux (
          {
            # the pre-commit hooks on all files: nix build .#checks.x86_64-linux.pre-commit -L
            pre-commit = preCommit;

            # scripts/visual in the build sandbox, with the package as
            # installed: nix build .#checks.x86_64-linux.visual -L
            # (the screenshots end up in the result)
            visual =
              runCommand "durst-visual-tests"
                {
                  src = lib.fileset.toSource {
                    root = ./.;
                    fileset = lib.fileset.unions [
                      ./scripts/visual
                      ./contrib/dbus
                    ];
                  };
                  nativeBuildInputs = visualTestTools ++ [
                    dbus
                    glib.bin # gdbus
                    systemdMinimal # busctl
                  ];
                  DURST_BIN_DIR = "${
                    symlinkJoin {
                      name = "durst-test-bin";
                      paths = [
                        packages.default
                        packages.test-tools
                      ];
                    }
                  }/bin";
                  DURST_TEST_FONTS = visualTestFonts;
                  LD_LIBRARY_PATH = lib.makeLibraryPath (runtimeLibs ++ [ pipewire ]);
                  # a loaded build machine is slow; only failures wait that long
                  WAIT = 30;
                }
                ''
                  cp -r $src/. .
                  chmod -R u+w .
                  patchShebangs scripts
                  export HOME=$TMPDIR LANG=C.UTF-8
                  scripts/visual/run-all.sh
                  cp -r target/visual $out
                '';
          }
          // lib.listToAttrs (
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
                { compositor = "kwin"; }
                { compositor = "river-classic"; }
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
          ]
          ++ visualTestTools
          ++ preCommit.enabledPackages;
          shellHook = preCommit.shellHook;
          buildInputs = runtimeLibs ++ [ pipewire ];
          LD_LIBRARY_PATH = "${lib.makeLibraryPath buildInputs}";
          DURST_TEST_FONTS = visualTestFonts;
        };
      }
    );
}
