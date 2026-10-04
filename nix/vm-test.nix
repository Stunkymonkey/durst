# durst on other compositors, in a NixOS VM: the same checks run on each
# compositor, so they only rely on what durst needs from any compositor
# (layer-shell, wl_output names, foreign-toplevel, idle-notify), with real
# input through QEMU's tablet instead of a compositor's virtual pointer.
#
#   nix build .#checks.x86_64-linux.vm-<compositor> -L
#
# The screenshots end up in the result.
{
  pkgs,
  lib,
  durst,
  compositor,
}:
let
  # each: the NixOS settings, the files in alice's home, and the command
  # started on tty1; the background must be one solid color for measure.py
  compositors = {
    sway = {
      module.programs.sway.enable = true;
      env.WLR_RENDERER = "pixman";
      files.".config/sway/config" = ''
        output * bg #000000 solid_color
        default_border none
      '';
      start = "sway";
    };
    labwc = {
      module.programs.labwc.enable = true;
      env.WLR_RENDERER = "pixman";
      files.".config/labwc/rc.xml" = ''
        <?xml version="1.0"?>
        <labwc_config><core><decoration>server</decoration></core></labwc_config>
      '';
      start = "labwc";
    };
    hyprland = {
      module.programs.hyprland.enable = true;
      env = { };
      files.".config/hypr/hyprland.conf" = ''
        monitor = , preferred, auto, 1
        misc {
          disable_hyprland_logo = true
          disable_splash_rendering = true
          background_color = 0x000000
        }
        animations {
          enabled = false
        }
        ecosystem {
          no_update_news = true
          no_donation_nag = true
        }
      '';
      start = "Hyprland";
    };
    # niri rejects software rendering on a tty (llvmpipe, as in the VM), so
    # it runs nested in sway, full screen; durst only talks to niri
    niri = {
      module = {
        programs.sway.enable = true;
        environment.systemPackages = [ pkgs.niri ];
      };
      env.WLR_RENDERER = "pixman";
      files.".config/sway/config" = ''
        output * bg #000000 solid_color
        default_border none
        for_window [title="niri"] fullscreen enable
        exec niri >/tmp/niri.log 2>&1
      '';
      files.".config/niri/config.kdl" = ''
        hotkey-overlay {
            skip-at-startup
        }
        animations {
            off
        }
        layout {
            background-color "#000000"
        }
      '';
      start = "sway";
      # sway's and niri's
      sockets = 2;
      log = "/tmp/niri.log";
    };
  };
  c = compositors.${compositor};

  config = pkgs.writeText "durst.toml" ''
    [general]
    idle_threshold = "2s"

    [urgency.normal]
    timeout = 0

    [[rule]]
    app_name = "^low$"
    anchor = "bottom-left"
  '';
  home = pkgs.runCommand "durst-test-home" { } (
    lib.concatStrings (
      lib.mapAttrsToList (path: text: ''
        install -Dm644 ${pkgs.writeText "file" text} $out/${path}
      '') (c.files // { ".config/durst/config.toml" = builtins.readFile config; })
    )
  );
in
pkgs.testers.runNixOSTest {
  name = "durst-${compositor}";

  nodes.machine = {
    imports = [ c.module ];

    users.users.alice = {
      isNormalUser = true;
      uid = 1000;
    };
    services.getty.autologinUser = "alice";
    programs.bash.loginShellInit = ''
      if [ "$(tty)" = /dev/tty1 ]; then
        cp -rT --no-preserve=mode ${home} ~
        ${lib.concatStrings (lib.mapAttrsToList (k: v: "export ${k}=${v}\n") c.env)}
        ${c.start} >/tmp/compositor.log 2>&1
      fi
    '';

    environment.systemPackages = [
      durst
      pkgs.libnotify
      pkgs.grim
      pkgs.foot
      (pkgs.python3.withPackages (ps: [ ps.pillow ]))
    ];
    environment.etc."durst-test/measure.py".source = ../scripts/visual/measure.py;
    fonts.packages = [ pkgs.dejavu_fonts ];

    hardware.graphics.enable = true;
    virtualisation = {
      memorySize = 2048;
      qemu.options = [ "-vga none -device virtio-gpu-pci" ];
    };
  };

  testScript = ''
    import json
    import shlex
    import time
    from typing import Any, cast

    ENV = (
        "export XDG_RUNTIME_DIR=/run/user/1000"
        " DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus"
        " WAYLAND_DISPLAY=$(cd /run/user/1000 && ls wayland-[0-9] | tail -1); "
    )


    def user(cmd):
        return machine.succeed(f"su - alice -c {shlex.quote(ENV + cmd)}")


    def info():
        return json.loads(user("durstctl info --json"))


    def boxes(name):
        """notification boxes (380 px wide) on a screenshot, and its size"""
        user(f"grim /tmp/{name}.png")
        machine.copy_from_machine(f"/tmp/{name}.png")
        m = json.loads(machine.succeed(f"python3 /etc/durst-test/measure.py /tmp/{name}.png"))
        return [b for b in m["boxes"] if b["w"] == 380], m["size"]


    def pointer(*events: Any):
        """input through QEMU's tablet, as a real device"""
        assert machine.qmp_client is not None
        machine.qmp_client.send("input-send-event", cast(dict[str, str], {"events": list(events)}))


    def move(x, y, size):
        # the tablet's absolute axes go from 0 to 0x7fff
        pointer(
            {"type": "abs", "data": {"axis": "x", "value": x * 0x7FFF // (size[0] - 1)}},
            {"type": "abs", "data": {"axis": "y", "value": y * 0x7FFF // (size[1] - 1)}},
        )


    def click(x, y, size):
        move(x, y, size)
        time.sleep(0.3)
        pointer({"type": "btn", "data": {"down": True, "button": "left"}})
        pointer({"type": "btn", "data": {"down": False, "button": "left"}})


    def notify(*args):
        return int(user("notify-send -p " + " ".join(shlex.quote(a) for a in args)))


    def shown():
        return [n["id"] for n in json.loads(user("durstctl notif list --json"))]


    def wait_info(key, value, timeout=30):
        retry(lambda _: info()[key] == value, timeout)


    start_all()
    # the compositor's socket, the last one if it runs nested
    machine.wait_until_succeeds(
        "test $(ls /run/user/1000/wayland-[0-9] | wc -l) -ge ${toString (c.sockets or 1)}", timeout=120
    )
    machine.sleep(3)
    # the packaged user unit
    user(
        "systemctl --user set-environment WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
        " ICED_BACKEND=tiny-skia RUST_LOG=info,durst=debug"
        " && systemctl --user start durst"
    )
    retry(lambda _: machine.execute(f"su - alice -c {shlex.quote(ENV + 'durstctl info')}")[0] == 0, 60)

    def logs():
        """what went wrong, for a failing check"""
        machine.execute("tail -40 ${c.log or "/tmp/compositor.log"} >&2")
        machine.execute("journalctl _SYSTEMD_USER_UNIT=durst.service --no-pager | grep -E 'WARN|ERROR|durst::' | tail -40 >&2")


    try:
        with subtest("outputs have names"):
            outputs = info()["outputs"]
            print(f"outputs: {outputs}")
            assert outputs, "no outputs"

        with subtest("a notification appears top right"):
            first = notify("First")
            machine.sleep(2)
            found, size = boxes("first")
            print(f"size {size}, boxes {found}")
            assert len(found) == 1, found
            b = found[0]
            assert b["x"] + b["w"] == size[0] - 20 and b["y"] == 20, b

        with subtest("a rule's anchor gets a stack of its own"):
            notify("-a", "low", "Second")
            machine.sleep(2)
            found, size = boxes("anchor")
            print(f"boxes {found}")
            assert len(found) == 2, found
            low = [b for b in found if b["x"] == 20]
            assert low and low[0]["y"] + low[0]["h"] == size[1] - 20, found

        with subtest("a click closes a notification"):
            b = [b for b in found if b["x"] != 20][0]
            click(b["x"] + b["w"] // 2, b["y"] + b["h"] // 2, size)
            retry(lambda _: first not in shown(), 10)

        with subtest("a fullscreen window is detected"):
            user("systemd-run --user --unit=fullscreen foot --fullscreen sleep 600")
            wait_info("fullscreen", True)
            user("systemctl --user stop fullscreen")
            wait_info("fullscreen", False)

        with subtest("idle, and active again on input"):
            wait_info("idle", True)
            move(100, 100, size)
            move(200, 200, size)
            wait_info("idle", False, 10)
    finally:
        logs()

  '';
}
