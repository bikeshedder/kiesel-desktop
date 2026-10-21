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

use crate::theme::{Appearance, Color, Scheme};

/// What the desktop asks of apps besides light and dark
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SystemPrefs {
    /// The desktop's accent colour, if it has one (GNOME 47 and later, KDE)
    pub accent: Option<Color>,
    /// Higher contrast
    pub high_contrast: bool,
    /// Fewer animations
    pub reduce_motion: bool,
    /// Larger (or smaller) text
    pub text_scale: f32,
}

impl Default for SystemPrefs {
    fn default() -> Self {
        Self {
            accent: None,
            high_contrast: false,
            reduce_motion: false,
            text_scale: 1.0,
        }
    }
}

impl SystemPrefs {
    /// `base` as the desktop wants it: its accent, high contrast (Kiesel's contrast
    /// 2), reduced motion, text scale
    pub fn apply(&self, base: &Appearance) -> Appearance {
        Appearance {
            accent_color: self.accent.unwrap_or(base.accent_color),
            contrast: if self.high_contrast {
                2.0
            } else {
                base.contrast
            },
            reduce_motion: base.reduce_motion || self.reduce_motion,
            text_scale: base.text_scale * self.text_scale,
            ..*base
        }
    }
}

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
    prefs: ArcRwSignal<SystemPrefs>,
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
        let prefs = ArcRwSignal::new(SystemPrefs::default());
        #[cfg(target_os = "linux")]
        portal::follow(system.clone(), prefs.clone());
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
            prefs,
            scheme,
            _save: None,
        }
    }

    /// The desktop's other appearance settings: accent, contrast, motion, text size
    pub fn prefs(&self) -> ArcSignal<SystemPrefs> {
        self.prefs.clone().into()
    }

    /// For tests and platforms that report these themselves
    pub fn set_prefs(&self, prefs: SystemPrefs) {
        if self.prefs.get_untracked() != prefs {
            self.prefs.set(prefs);
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
    use reactive_graph::traits::{GetUntracked, Set};
    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{OwnedValue, Value};

    use super::SystemPrefs;
    use crate::theme::{Color, Scheme};

    const APPEARANCE: &str = "org.freedesktop.appearance";
    /// GNOME's own keys, which its portal also serves: animations and text size
    const GNOME: &str = "org.gnome.desktop.interface";
    const KEYS: [(&str, &str); 6] = [
        (APPEARANCE, "color-scheme"),
        (APPEARANCE, "accent-color"),
        (APPEARANCE, "contrast"),
        (APPEARANCE, "reduced-motion"),
        (GNOME, "enable-animations"),
        (GNOME, "text-scaling-factor"),
    ];

    /// Reads the settings now (so the first frame is right) and follows their
    /// changes on a thread; without a portal nothing is known and defaults hold
    pub fn follow(system: ArcRwSignal<Option<Scheme>>, prefs: ArcRwSignal<SystemPrefs>) {
        let Ok(conn) = Connection::session() else {
            return;
        };
        let Ok(proxy) = settings(&conn) else { return };
        // subscribe before reading, so no change slips through in between
        let Ok(changes) = proxy.receive_signal("SettingChanged") else {
            return;
        };
        let mut now = SystemPrefs::default();
        for (ns, key) in KEYS {
            if let Some(value) = read(&proxy, ns, key) {
                apply(&mut now, &system, ns, key, &value);
            }
        }
        prefs.set(now);
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
                    let mut p = prefs.get_untracked();
                    apply(&mut p, &system, &ns, &key, &value);
                    if p != prefs.get_untracked() {
                        prefs.set(p);
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

    fn read(proxy: &Proxy<'_>, ns: &str, key: &str) -> Option<OwnedValue> {
        // ReadOne (portal v2); older portals only have Read, which wraps the value once more
        proxy
            .call("ReadOne", &(ns, key))
            .or_else(|_| proxy.call("Read", &(ns, key)))
            .ok()
    }

    fn apply(
        prefs: &mut SystemPrefs,
        system: &ArcRwSignal<Option<Scheme>>,
        ns: &str,
        key: &str,
        value: &Value<'_>,
    ) {
        let value = unwrap(value);
        match (ns, key) {
            // 1 = prefers dark, 2 = prefers light, 0 = no preference
            (APPEARANCE, "color-scheme") => {
                let scheme = match value {
                    Value::U32(1) => Some(Scheme::Dark),
                    Value::U32(2) => Some(Scheme::Light),
                    _ => None,
                };
                if system.get_untracked() != scheme {
                    system.set(scheme);
                }
            }
            // (r, g, b) in 0 … 1; outside that range: no accent
            (APPEARANCE, "accent-color") => {
                prefs.accent = match value {
                    Value::Structure(s) => {
                        let c: Vec<f64> = s
                            .fields()
                            .iter()
                            .filter_map(|f| f64::try_from(f).ok())
                            .collect();
                        (c.len() == 3 && c.iter().all(|v| (0.0..=1.0).contains(v)))
                            .then(|| Color::new([c[0] as f32, c[1] as f32, c[2] as f32, 1.0]))
                    }
                    _ => None,
                };
            }
            // 1 = higher contrast
            (APPEARANCE, "contrast") => prefs.high_contrast = matches!(value, Value::U32(1)),
            // 1 = reduced motion
            (APPEARANCE, "reduced-motion") => prefs.reduce_motion = matches!(value, Value::U32(1)),
            (GNOME, "enable-animations") => {
                if let Value::Bool(on) = value {
                    prefs.reduce_motion = !on;
                }
            }
            (GNOME, "text-scaling-factor") => {
                if let Value::F64(f) = value
                    && (0.5..=3.0).contains(f)
                {
                    prefs.text_scale = *f as f32;
                }
            }
            _ => {}
        }
    }

    fn unwrap<'a>(v: &'a Value<'a>) -> &'a Value<'a> {
        match v {
            Value::Value(inner) => unwrap(inner),
            v => v,
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
