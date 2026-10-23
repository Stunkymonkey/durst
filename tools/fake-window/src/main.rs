//! A plain window for the visual test harness that asks to be activated
//! (xdg-activation-v1), like an application does with the token from a
//! notification's `ActivationToken` signal.
//!
//! usage: fake-window APP_ID, then one activation token per line on stdin.
//! Prints "mapped" once the window is shown and "activate <token>" for each
//! request; whether it worked shows in the compositor's focus.

use std::fs::File;
use std::io::{BufRead, Write};
use std::os::fd::AsFd;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

const SIZE: i32 = 64;

struct State {
    configured: bool,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        wm: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.configured = true;
        }
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);
delegate_noop!(State: ignore xdg_toplevel::XdgToplevel);
delegate_noop!(State: ignore XdgActivationV1);

fn main() {
    let app_id = std::env::args().nth(1).expect("usage: fake-window APP_ID");
    let conn = Connection::connect_to_env().expect("wayland connection");
    let (globals, mut queue) = registry_queue_init::<State>(&conn).expect("registry");
    let qh = queue.handle();
    let compositor: wl_compositor::WlCompositor =
        globals.bind(&qh, 4..=6, ()).expect("wl_compositor");
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).expect("wl_shm");
    let wm: xdg_wm_base::XdgWmBase = globals.bind(&qh, 1..=6, ()).expect("xdg_wm_base");
    let activation: XdgActivationV1 = globals.bind(&qh, 1..=1, ()).expect("xdg_activation_v1");

    let surface = compositor.create_surface(&qh, ());
    let xdg = wm.get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg.get_toplevel(&qh, ());
    toplevel.set_app_id(app_id);
    surface.commit();
    let mut state = State { configured: false };
    while !state.configured {
        queue.blocking_dispatch(&mut state).expect("dispatch");
    }

    // a gray buffer, so the window is mapped
    let dir = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR");
    let path = format!("{dir}/fake-window-{}", std::process::id());
    let mut file = File::options()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&path)
        .expect("shm file");
    std::fs::remove_file(&path).ok();
    file.write_all(&[0x80; (SIZE * SIZE * 4) as usize])
        .expect("write shm");
    let pool = shm.create_pool(file.as_fd(), SIZE * SIZE * 4, &qh, ());
    let buffer = pool.create_buffer(0, SIZE, SIZE, SIZE * 4, wl_shm::Format::Xrgb8888, &qh, ());
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    queue.roundtrip(&mut state).expect("roundtrip");
    println!("mapped");

    for line in std::io::stdin().lock().lines() {
        let token = line.expect("stdin");
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        println!("activate {token}");
        activation.activate(token.to_owned(), &surface);
        queue.roundtrip(&mut state).expect("roundtrip");
    }
}
