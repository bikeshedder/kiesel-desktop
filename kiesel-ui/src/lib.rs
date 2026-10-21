//! Kiesel UI as a native Rust toolkit: Soft UI drawn by vello_cpu in a winit window.

mod a11y;
pub mod anim;
pub mod button;
pub mod card;
pub mod clipboard;
pub mod control;
pub mod flex;
pub mod i18n;
pub mod icon;
pub mod label;
mod layout;
pub mod paint;
pub mod popup;
pub mod reactive;
pub mod scroll;
pub mod shell;
pub mod switch;
pub mod text;
pub mod text_field;
pub mod theme;
pub mod theme_switch;
pub mod theming;
pub mod widget;

pub use button::Button;
pub use card::Card;
pub use control::{For, Show};
pub use flex::{Align, Flex, Justify};
pub use i18n::{IntoText, t, t_with};
pub use label::Label;
pub use popup::{Placement, Popover, Popup};
pub use scroll::Scroll;
pub use shell::{Options, run};
pub use switch::Switch;
pub use text_field::TextField;
pub use theme::{Appearance, ControlSize, Scheme, TextRole, Theme, space};
pub use theme_switch::{IconToggle, ThemeSwitch};
pub use theming::{ThemeMode, Theming};
pub use widget::{Change, Event, Widget, WidgetId};

pub use reactive::{MountCx, Watch};
pub use reactive_graph::signal::ArcRwSignal;
pub use reactive_graph::traits::{Get, GetUntracked, Set, Track, Update, With, WithUntracked};
pub use reactive_graph::wrappers::read::ArcSignal;
pub use vello_cpu::kurbo;
