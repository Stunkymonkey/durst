//! A long-lived virtual pointer (wlr-virtual-pointer-unstable-v1) for the
//! visual test harness. Headless compositors have no input devices, and a
//! pointer that only exists for a single click (like `wlrctl`) is gone before
//! clients have bound `wl_pointer`.
//!
//! usage: vpointer WIDTH HEIGHT, then commands on stdin, one per line:
//!   move X Y        absolute position in output pixels
//!   click BUTTON    left | right | middle
//!   scroll DY       vertical scroll, positive is down

use std::io::BufRead;
use std::time::Instant;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_pointer, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

struct State;

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

macro_rules! ignore_events {
    ($($t:ty),*) => {$(
        impl Dispatch<$t, ()> for State {
            fn event(_: &mut Self, _: &$t, _: <$t as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        }
    )*};
}
ignore_events!(
    wl_seat::WlSeat,
    ZwlrVirtualPointerManagerV1,
    ZwlrVirtualPointerV1
);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<u32> = std::env::args()
        .skip(1)
        .map(|a| a.parse())
        .collect::<Result<_, _>>()?;
    let [width, height] = args[..] else {
        return Err("usage: vpointer WIDTH HEIGHT".into());
    };

    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=7, ())?;
    let manager: ZwlrVirtualPointerManagerV1 = globals.bind(&qh, 1..=2, ())?;
    let pointer = manager.create_virtual_pointer(Some(&seat), &qh, ());
    queue.roundtrip(&mut State)?;

    let start = Instant::now();
    let time = || start.elapsed().as_millis() as u32;
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let words: Vec<&str> = line.split_whitespace().collect();
        match words[..] {
            ["move", x, y] => {
                pointer.motion_absolute(time(), x.parse()?, y.parse()?, width, height);
            }
            ["click", button] => {
                let code = match button {
                    "left" => BTN_LEFT,
                    "right" => BTN_RIGHT,
                    "middle" => BTN_MIDDLE,
                    _ => return Err(format!("unknown button {button:?}").into()),
                };
                pointer.button(time(), code, wl_pointer::ButtonState::Pressed);
                pointer.frame();
                pointer.button(time(), code, wl_pointer::ButtonState::Released);
            }
            ["scroll", dy] => {
                pointer.axis(time(), wl_pointer::Axis::VerticalScroll, dy.parse()?);
            }
            [] => continue,
            _ => return Err(format!("unknown command {line:?}").into()),
        }
        pointer.frame();
        queue.roundtrip(&mut State)?;
    }
    Ok(())
}
