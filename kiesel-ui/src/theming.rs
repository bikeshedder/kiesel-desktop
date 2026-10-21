//! Light, dark, or whatever the system prefers – the user's choice, remembered.
//!
//! [`Theming`] holds three signals: the mode the user chose ([`ThemeMode`],
//! saved per application in the config directory), the system's preference,
//! and the scheme that follows from both, which the shell shows (cross-fading
//! when it changes) and widgets like the `ThemeSwitch` read.
//!
//! The system preference comes from the XDG desktop portal on Linux
//! (`org.freedesktop.appearance color-scheme` – winit doesn't read it on
//! Wayland), from winit's theme events elsewhere. `KIESEL_THEME=light|dark|system`
//! picks the mode for one run, without saving it.

use std::path::PathBuf;
use std::sync::Arc;

use reactive_graph::effect::ImmediateEffect;
use reactive_graph::signal::ArcRwSignal;
use reactive_graph::traits::{Get, GetUntracked, Set};
use reactive_graph::wrappers::read::ArcSignal;

use crate::theme::Scheme;

/// What the user chose
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    /// Follow the system's light/dark preference
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct Theming {
    mode: ArcRwSignal<ThemeMode>,
    system: ArcRwSignal<Option<Scheme>>,
    scheme: ArcSignal<Scheme>,
    /// Saves the mode when it changes (kept alive with the handle)
    _save: Option<Arc<ImmediateEffect>>,
}

impl Theming {
    /// Remembers the mode for `app_id` (e.g. `"de.example.Notes"`) in the user's
    /// config directory, and follows the system's preference
    pub fn new(app_id: &str) -> Self {
        let file = config_dir().map(|d| d.join(app_id).join("kiesel.conf"));
        let stored = file.as_ref().and_then(load);
        let mut theming = Self::build(env_mode().or(stored).unwrap_or_default());
        if let Some(file) = file {
            let mode = theming.mode.clone();
            let first = std::sync::atomic::AtomicBool::new(true);
            theming._save = Some(Arc::new(ImmediateEffect::new_isomorphic(move || {
                let m = mode.get();
                if !first.swap(false, std::sync::atomic::Ordering::AcqRel) {
                    save(&file, m);
                }
            })));
        }
        theming
    }

    /// Follows the system, remembers nothing (tests, small tools)
    pub fn unsaved() -> Self {
        Self::build(env_mode().unwrap_or_default())
    }

    fn build(mode: ThemeMode) -> Self {
        let mode = ArcRwSignal::new(mode);
        let system = ArcRwSignal::new(None);
        #[cfg(target_os = "linux")]
        portal::follow(system.clone());
        let scheme = ArcSignal::derive({
            let (mode, system) = (mode.clone(), system.clone());
            move || match mode.get() {
                ThemeMode::Light => Scheme::Light,
                ThemeMode::Dark => Scheme::Dark,
                ThemeMode::System => system.get().unwrap_or(Scheme::Light),
            }
        });
        Self {
            mode,
            system,
            scheme,
            _save: None,
        }
    }

    /// The user's choice; set it to switch
    pub fn mode(&self) -> ArcRwSignal<ThemeMode> {
        self.mode.clone()
    }

    /// The scheme on screen
    pub fn scheme(&self) -> ArcSignal<Scheme> {
        self.scheme.clone()
    }

    /// The system's preference, if it states one
    pub fn system(&self) -> ArcSignal<Option<Scheme>> {
        self.system.clone().into()
    }

    /// Where the platform reports the preference itself (winit on Windows and macOS)
    pub(crate) fn set_system(&self, scheme: Option<Scheme>) {
        if self.system.get_untracked() != scheme {
            self.system.set(scheme);
        }
    }
}

fn env_mode() -> Option<ThemeMode> {
    std::env::var("KIESEL_THEME")
        .ok()
        .and_then(|v| ThemeMode::parse(&v))
}

/// `$XDG_CONFIG_HOME`, else `~/.config` (on Windows `%APPDATA%`)
fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("APPDATA").map(PathBuf::from);
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

/// `theme = dark` – one setting per line; other lines are kept for later settings
fn load(file: &PathBuf) -> Option<ThemeMode> {
    let text = std::fs::read_to_string(file).ok()?;
    text.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == "theme")
        .and_then(|(_, v)| ThemeMode::parse(v))
}

fn save(file: &PathBuf, mode: ThemeMode) {
    let mut lines: Vec<String> = std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .filter(|l| l.split_once('=').is_none_or(|(k, _)| k.trim() != "theme"))
        .map(str::to_owned)
        .collect();
    lines.push(format!("theme = {}", mode.as_str()));
    // a failed save costs the choice for the next start, nothing more
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(file, lines.join("\n") + "\n");
}

#[cfg(target_os = "linux")]
mod portal {
    //! The XDG desktop portal's appearance settings, over D-Bus
    use reactive_graph::signal::ArcRwSignal;
    use reactive_graph::traits::Set;
    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{OwnedValue, Value};

    use crate::theme::Scheme;

    const NAMESPACE: &str = "org.freedesktop.appearance";
    const KEY: &str = "color-scheme";

    /// Reads the preference now (so the first frame is right) and follows its
    /// changes on a thread; without a portal the preference stays unknown
    pub fn follow(system: ArcRwSignal<Option<Scheme>>) {
        let Ok(conn) = Connection::session() else {
            return;
        };
        let Ok(proxy) = settings(&conn) else { return };
        // subscribe before reading, so no change slips through in between
        let Ok(changes) = proxy.receive_signal("SettingChanged") else {
            return;
        };
        system.set(read(&proxy));
        let _ = std::thread::Builder::new()
            .name("kiesel-portal".into())
            .spawn(move || {
                let _keep = (&conn, &proxy);
                for msg in changes {
                    let Ok((ns, key, value)) =
                        msg.body().deserialize::<(String, String, OwnedValue)>()
                    else {
                        continue;
                    };
                    if ns == NAMESPACE && key == KEY {
                        system.set(scheme_of(&value));
                    }
                }
            });
    }

    fn settings(conn: &Connection) -> zbus::Result<Proxy<'static>> {
        Proxy::new(
            conn,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )
    }

    fn read(proxy: &Proxy<'_>) -> Option<Scheme> {
        // ReadOne (portal v2); older portals only have Read, which wraps the value once more
        let value: OwnedValue = proxy
            .call("ReadOne", &(NAMESPACE, KEY))
            .or_else(|_| proxy.call("Read", &(NAMESPACE, KEY)))
            .ok()?;
        scheme_of(&value)
    }

    /// 1 = prefers dark, 2 = prefers light, 0 = no preference
    fn scheme_of(value: &Value<'_>) -> Option<Scheme> {
        match value {
            Value::Value(inner) => scheme_of(inner),
            Value::U32(1) => Some(Scheme::Dark),
            Value::U32(2) => Some(Scheme::Light),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mode_decides_unless_it_follows_the_system() {
        let t = Theming::build(ThemeMode::System);
        t.set_system(Some(Scheme::Dark));
        assert_eq!(t.scheme().get_untracked(), Scheme::Dark);
        t.mode().set(ThemeMode::Light);
        assert_eq!(t.scheme().get_untracked(), Scheme::Light);
        t.set_system(Some(Scheme::Light));
        t.set_system(Some(Scheme::Dark));
        assert_eq!(
            t.scheme().get_untracked(),
            Scheme::Light,
            "a choice beats the system"
        );
        t.mode().set(ThemeMode::System);
        assert_eq!(t.scheme().get_untracked(), Scheme::Dark);
        t.set_system(None);
        assert_eq!(
            t.scheme().get_untracked(),
            Scheme::Light,
            "no preference: light"
        );
    }

    #[test]
    fn the_choice_is_saved_and_other_lines_kept() {
        let dir = std::env::temp_dir().join(format!("kiesel-theming-{}", std::process::id()));
        let file = dir.join("app").join("kiesel.conf");
        save(&file, ThemeMode::Dark);
        assert_eq!(load(&file), Some(ThemeMode::Dark));
        std::fs::write(&file, "accent = #707ed4\ntheme = dark\n").unwrap();
        save(&file, ThemeMode::System);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "accent = #707ed4\ntheme = system\n"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
