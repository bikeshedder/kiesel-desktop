//! A virtual pointer and keyboard that live as long as the headless session.
//!
//! `wlrctl` and `wtype` create a device per command and drop it right away, so
//! the seat has pointer/keyboard capability only for an instant and clients
//! bind `wl_pointer`/`wl_keyboard` too late to see anything. This helper keeps
//! one of each and reads commands, one per line, from a FIFO; after each it
//! writes `ok` (or `error: …`) to the ack FIFO.
//!
//!   kiesel-input <width> <height> <command-fifo> <ack-fifo>
//!
//! Pointer (logical coordinates): `move x y`, `down [button]`, `up [button]`,
//! `click x y [button]`, `scroll dy` – buttons: left, right, middle.
//!
//! Keyboard (us layout): `key <combo>`, `keydown <combo>`, `keyup <combo>`,
//! `type <text>` – a combo is xkb key names joined by `+`, e.g. `Tab`,
//! `shift+Tab`, `ctrl+a`, `space`, `Return`, `Escape`.
//!
//! Input method (input-method-v2, which the compositor relays to the focused
//! client's text-input-v3): `ime preedit <text>`, `ime commit <text>`,
//! `ime clear`, `ime delete <before> <after>` – an error unless a client has
//! text input switched on right now.

use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsFd;
use std::time::{SystemTime, UNIX_EPOCH};

use wayland_client::protocol::{wl_keyboard, wl_pointer, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_manager_v2 as im_manager, zwp_input_method_v2 as im,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1 as vk_manager, zwp_virtual_keyboard_v1 as vk,
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1 as manager, zwlr_virtual_pointer_v1 as vp,
};
use xkbcommon::xkb;

#[derive(Default)]
struct State {
    seat: Option<wl_seat::WlSeat>,
    manager: Option<manager::ZwlrVirtualPointerManagerV1>,
    keyboards: Option<vk_manager::ZwpVirtualKeyboardManagerV1>,
    input_methods: Option<im_manager::ZwpInputMethodManagerV2>,
    ime: ImeState,
}

/// What the compositor told the input method; applied on `done`
#[derive(Default)]
struct ImeState {
    pending_active: bool,
    active: bool,
    /// Number of `done` events: the serial `commit` must carry
    serial: u32,
    unavailable: bool,
}

impl Dispatch<im::ZwpInputMethodV2, ()> for State {
    fn event(
        state: &mut Self,
        _: &im::ZwpInputMethodV2,
        event: im::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let ime = &mut state.ime;
        match event {
            im::Event::Activate => ime.pending_active = true,
            im::Event::Deactivate => ime.pending_active = false,
            im::Event::Done => {
                ime.active = ime.pending_active;
                ime.serial += 1;
            }
            im::Event::Unavailable => ime.unavailable = true,
            _ => {}
        }
    }
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
                "zwp_virtual_keyboard_manager_v1" => {
                    state.keyboards = Some(registry.bind(name, 1, qh, ()))
                }
                "zwp_input_method_manager_v2" => {
                    state.input_methods = Some(registry.bind(name, 1, qh, ()))
                }
                _ => {}
            }
        }
    }
}

wayland_client::delegate_noop!(State: ignore wl_seat::WlSeat);
wayland_client::delegate_noop!(State: manager::ZwlrVirtualPointerManagerV1);
wayland_client::delegate_noop!(State: vp::ZwlrVirtualPointerV1);
wayland_client::delegate_noop!(State: vk_manager::ZwpVirtualKeyboardManagerV1);
wayland_client::delegate_noop!(State: vk::ZwpVirtualKeyboardV1);
wayland_client::delegate_noop!(State: im_manager::ZwpInputMethodManagerV2);

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
}

/// A key on the us keymap: its xkb keycode and whether it needs Shift
#[derive(Clone, Copy)]
struct Key {
    code: xkb::Keycode,
    shift: bool,
}

struct Keyboard {
    vk: vk::ZwpVirtualKeyboardV1,
    keymap: xkb::Keymap,
    state: xkb::State,
}

impl Keyboard {
    fn new(vk: vk::ZwpVirtualKeyboardV1) -> Result<Self, String> {
        let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap =
            xkb::Keymap::new_from_names(&ctx, "", "", "us", "", None, xkb::KEYMAP_COMPILE_NO_FLAGS)
                .ok_or("cannot compile the us keymap (xkb-data missing?)")?;
        // the keymap goes to the compositor as a file; it is read up to the NUL
        let mut text = keymap
            .get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1)
            .into_bytes();
        text.push(0);
        let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
        let path = format!("{dir}/kiesel-input-keymap-{}", std::process::id());
        let mut file = std::fs::File::create_new(&path).map_err(|e| e.to_string())?;
        file.write_all(&text).map_err(|e| e.to_string())?;
        let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&path);
        vk.keymap(
            wl_keyboard::KeymapFormat::XkbV1.into(),
            file.as_fd(),
            text.len() as u32,
        );
        let state = xkb::State::new(&keymap);
        Ok(Self { vk, keymap, state })
    }

    /// Finds the key producing `sym`, on level 0 or (with Shift) level 1
    fn find(&self, sym: xkb::Keysym) -> Option<Key> {
        let (min, max) = (
            self.keymap.min_keycode().raw(),
            self.keymap.max_keycode().raw(),
        );
        for level in 0..2 {
            for raw in min..=max {
                let code = xkb::Keycode::new(raw);
                if self
                    .keymap
                    .key_get_syms_by_level(code, 0, level)
                    .contains(&sym)
                {
                    return Some(Key {
                        code,
                        shift: level == 1,
                    });
                }
            }
        }
        None
    }

    fn named(&self, name: &str) -> Result<Key, String> {
        let alias = match name.to_ascii_lowercase().as_str() {
            "shift" => "Shift_L",
            "ctrl" | "control" => "Control_L",
            "alt" => "Alt_L",
            "super" | "logo" => "Super_L",
            "enter" => "Return",
            "esc" => "Escape",
            "space" => "space",
            _ => name,
        };
        let mut sym = xkb::keysym_from_name(alias, xkb::KEYSYM_NO_FLAGS);
        if sym.raw() == 0 {
            sym = xkb::keysym_from_name(alias, xkb::KEYSYM_CASE_INSENSITIVE);
        }
        if sym.raw() == 0 {
            return Err(format!("unknown key {name}"));
        }
        self.find(sym)
            .ok_or(format!("{name} is not on the us keymap"))
    }

    fn combo(&self, spec: &str) -> Result<Vec<Key>, String> {
        let mut keys = Vec::new();
        for part in spec.split('+') {
            let key = self.named(part)?;
            if key.shift {
                keys.push(self.named("Shift_L")?);
            }
            keys.push(Key {
                shift: false,
                ..key
            });
        }
        Ok(keys)
    }

    fn send(&mut self, key: Key, down: bool) {
        let direction = if down {
            xkb::KeyDirection::Down
        } else {
            xkb::KeyDirection::Up
        };
        // wayland key codes are evdev codes: xkb keycode - 8
        self.vk.key(now_ms(), key.code.raw() - 8, u32::from(down));
        self.state.update_key(key.code, direction);
        self.vk.modifiers(
            self.state.serialize_mods(xkb::STATE_MODS_DEPRESSED),
            self.state.serialize_mods(xkb::STATE_MODS_LATCHED),
            self.state.serialize_mods(xkb::STATE_MODS_LOCKED),
            self.state.serialize_layout(xkb::STATE_LAYOUT_EFFECTIVE),
        );
    }

    fn press(&mut self, keys: &[Key]) {
        for &k in keys {
            self.send(k, true);
        }
    }

    fn release(&mut self, keys: &[Key]) {
        for &k in keys.iter().rev() {
            self.send(k, false);
        }
    }
}

struct Devices {
    pointer: Pointer,
    keyboard: Keyboard,
    ime: Option<im::ZwpInputMethodV2>,
}

impl Devices {
    fn run(
        &mut self,
        line: &str,
        queue: &mut EventQueue<State>,
        state: &mut State,
    ) -> Result<(), String> {
        let (cmd, rest) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
        let mut words = rest.split_whitespace();
        let num = |w: Option<&str>| {
            w.ok_or("missing number")?
                .parse::<f64>()
                .map_err(|e| e.to_string())
        };
        let mut sync = |state: &mut State| {
            queue
                .roundtrip(state)
                .map(|_| ())
                .map_err(|e| e.to_string())
        };
        match cmd {
            "move" => self.pointer.move_to(num(words.next())?, num(words.next())?),
            "down" => self.pointer.button(button_code(words.next())?, true),
            "up" => self.pointer.button(button_code(words.next())?, false),
            "click" => {
                let (x, y) = (num(words.next())?, num(words.next())?);
                let code = button_code(words.next())?;
                self.pointer.move_to(x, y);
                // let the client see the motion before the press, like a hand would
                sync(state)?;
                self.pointer.button(code, true);
                sync(state)?;
                self.pointer.button(code, false);
            }
            "scroll" => {
                self.pointer.vp.axis(
                    now_ms(),
                    wl_pointer::Axis::VerticalScroll,
                    num(words.next())?,
                );
                self.pointer.vp.frame();
            }
            "key" => {
                let keys = self.keyboard.combo(rest.trim())?;
                self.keyboard.press(&keys);
                sync(state)?;
                self.keyboard.release(&keys);
            }
            "keydown" => {
                let keys = self.keyboard.combo(rest.trim())?;
                self.keyboard.press(&keys);
            }
            "keyup" => {
                let keys = self.keyboard.combo(rest.trim())?;
                self.keyboard.release(&keys);
            }
            "type" => {
                for ch in rest.chars() {
                    let key = self
                        .keyboard
                        .find(xkb::utf32_to_keysym(u32::from(ch)))
                        .ok_or(format!("{ch:?} is not on the us keymap"))?;
                    let keys = if key.shift {
                        vec![
                            self.keyboard.named("Shift_L")?,
                            Key {
                                shift: false,
                                ..key
                            },
                        ]
                    } else {
                        vec![key]
                    };
                    self.keyboard.press(&keys);
                    self.keyboard.release(&keys);
                    sync(state)?;
                }
            }
            "ime" => {
                // events first: the client may just have switched text input on or off
                sync(state)?;
                let im = self
                    .ime
                    .as_ref()
                    .ok_or("no input method (another one is running?)")?;
                if state.ime.unavailable {
                    return Err("input method unavailable: another one is registered".into());
                }
                if !state.ime.active {
                    return Err("no client has text input on".into());
                }
                let (verb, text) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
                match verb {
                    "preedit" => {
                        let end = text.len() as i32;
                        im.set_preedit_string(text.to_string(), end, end);
                    }
                    "commit" => im.commit_string(text.to_string()),
                    "clear" => im.set_preedit_string(String::new(), 0, 0),
                    "delete" => {
                        let mut n = text.split_whitespace().map(str::parse::<u32>);
                        let (Some(Ok(before)), Some(Ok(after))) = (n.next(), n.next()) else {
                            return Err("ime delete <before> <after>".into());
                        };
                        im.delete_surrounding_text(before, after);
                    }
                    other => return Err(format!("unknown ime command {other}")),
                }
                im.commit(state.ime.serial);
            }
            "" => return Ok(()),
            other => return Err(format!("unknown command {other}")),
        }
        sync(state)
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
    let (Some(seat), Some(manager), Some(keyboards)) =
        (&state.seat, &state.manager, &state.keyboards)
    else {
        return Err("compositor lacks wl_seat or the virtual pointer/keyboard protocols".into());
    };
    let mut devices = Devices {
        pointer: Pointer {
            vp: manager.create_virtual_pointer(Some(seat), &qh, ()),
            extent: (w.parse()?, h.parse()?),
        },
        keyboard: Keyboard::new(keyboards.create_virtual_keyboard(seat, &qh, ()))?,
        ime: state
            .input_methods
            .as_ref()
            .map(|m| m.get_input_method(seat, &qh, ())),
    };
    queue.roundtrip(&mut state)?;

    // Reopen the FIFO whenever the writer closes it: one connection per command
    loop {
        let reader = BufReader::new(std::fs::File::open(commands)?);
        for line in reader.lines() {
            let line = line?;
            let reply = match devices.run(&line, &mut queue, &mut state) {
                Ok(()) => "ok".to_string(),
                Err(e) => format!("error: {e}"),
            };
            let mut ack = std::fs::OpenOptions::new().write(true).open(acks)?;
            writeln!(ack, "{reply}")?;
        }
    }
}
