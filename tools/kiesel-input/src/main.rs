//! A virtual pointer that lives as long as the headless session.
//!
//! `wlrctl` creates a pointer per command and drops it right away, so the seat
//! only has pointer capability for an instant and clients bind `wl_pointer` too
//! late to see anything. This helper keeps one pointer and reads commands, one
//! per line, from a FIFO; after each it writes `ok` (or `error: …`) to the ack FIFO.
//!
//!   kiesel-input <width> <height> <command-fifo> <ack-fifo>
//!
//! Commands (logical coordinates): `move x y`, `down [button]`, `up [button]`,
//! `click x y [button]`, `scroll dy` – buttons: left, right, middle.

use std::io::{BufRead, BufReader, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use wayland_client::protocol::{wl_pointer, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1 as manager, zwlr_virtual_pointer_v1 as vp,
};

#[derive(Default)]
struct State {
    seat: Option<wl_seat::WlSeat>,
    manager: Option<manager::ZwlrVirtualPointerManagerV1>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind(name, version.min(7), qh, ()))
                }
                "zwlr_virtual_pointer_manager_v1" => {
                    state.manager = Some(registry.bind(name, version.min(2), qh, ()))
                }
                _ => {}
            }
        }
    }
}

wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(State: manager::ZwlrVirtualPointerManagerV1);
wayland_client::delegate_noop!(State: vp::ZwlrVirtualPointerV1);

fn now_ms() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u32)
}

/// Linux input event codes (BTN_LEFT …)
fn button_code(name: Option<&str>) -> Result<u32, String> {
    match name.unwrap_or("left") {
        "left" => Ok(0x110),
        "right" => Ok(0x111),
        "middle" => Ok(0x112),
        other => Err(format!("unknown button {other}")),
    }
}

struct Pointer {
    vp: vp::ZwlrVirtualPointerV1,
    extent: (u32, u32),
}

impl Pointer {
    fn move_to(&self, x: f64, y: f64) {
        let (w, h) = self.extent;
        // motion_absolute takes integers in [0, extent]; logical pixels map 1:1
        self.vp.motion_absolute(
            now_ms(),
            x.clamp(0.0, f64::from(w)) as u32,
            y.clamp(0.0, f64::from(h)) as u32,
            w,
            h,
        );
        self.vp.frame();
    }

    fn button(&self, code: u32, pressed: bool) {
        let state = if pressed {
            wl_pointer::ButtonState::Pressed
        } else {
            wl_pointer::ButtonState::Released
        };
        self.vp.button(now_ms(), code, state);
        self.vp.frame();
    }

    fn run(
        &self,
        line: &str,
        queue: &mut EventQueue<State>,
        state: &mut State,
    ) -> Result<(), String> {
        let mut words = line.split_whitespace();
        let num = |w: Option<&str>| {
            w.ok_or("missing number")?
                .parse::<f64>()
                .map_err(|e| e.to_string())
        };
        match words.next() {
            Some("move") => self.move_to(num(words.next())?, num(words.next())?),
            Some("down") => self.button(button_code(words.next())?, true),
            Some("up") => self.button(button_code(words.next())?, false),
            Some("click") => {
                let (x, y) = (num(words.next())?, num(words.next())?);
                let code = button_code(words.next())?;
                self.move_to(x, y);
                // let the client see the motion before the press, like a hand would
                queue.roundtrip(state).map_err(|e| e.to_string())?;
                self.button(code, true);
                queue.roundtrip(state).map_err(|e| e.to_string())?;
                self.button(code, false);
            }
            Some("scroll") => {
                self.vp.axis(
                    now_ms(),
                    wl_pointer::Axis::VerticalScroll,
                    num(words.next())?,
                );
                self.vp.frame();
            }
            Some(other) => return Err(format!("unknown command {other}")),
            None => return Ok(()),
        }
        queue.roundtrip(state).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let [_, w, h, commands, acks] = args.as_slice() else {
        return Err("usage: kiesel-input <width> <height> <command-fifo> <ack-fifo>".into());
    };
    let conn = Connection::connect_to_env()?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut state = State::default();
    queue.roundtrip(&mut state)?;
    let (Some(seat), Some(manager)) = (&state.seat, &state.manager) else {
        return Err("compositor lacks wl_seat or zwlr_virtual_pointer_manager_v1".into());
    };
    let pointer = Pointer {
        vp: manager.create_virtual_pointer(Some(seat), &qh, ()),
        extent: (w.parse()?, h.parse()?),
    };
    queue.roundtrip(&mut state)?;

    // Reopen the FIFO whenever the writer closes it: one connection per command
    loop {
        let reader = BufReader::new(std::fs::File::open(commands)?);
        for line in reader.lines() {
            let line = line?;
            let reply = match pointer.run(&line, &mut queue, &mut state) {
                Ok(()) => "ok".to_string(),
                Err(e) => format!("error: {e}"),
            };
            let mut ack = std::fs::OpenOptions::new().write(true).open(acks)?;
            writeln!(ack, "{reply}")?;
        }
    }
}
