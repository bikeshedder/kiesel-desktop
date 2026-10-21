//! Kiesel UI as a native Rust toolkit: Soft UI drawn by vello_cpu in a winit window.

mod a11y;
pub mod anim;
pub mod button;
pub mod card;
pub mod clipboard;
pub mod flex;
pub mod label;
mod layout;
pub mod paint;
pub mod reactive;
pub mod shell;
pub mod text;
pub mod text_field;
pub mod theme;
pub mod widget;

pub use button::Button;
pub use card::Card;
pub use flex::{Align, Flex, Justify};
pub use label::Label;
pub use shell::{Options, run};
pub use text_field::TextField;
pub use theme::{Appearance, Scheme, TextRole, Theme, space};
pub use widget::{Change, Event, Widget, WidgetId};

pub use reactive::{MountCx, Watch};
pub use reactive_graph::signal::ArcRwSignal;
pub use reactive_graph::traits::{Get, GetUntracked, Set, Track, Update, With, WithUntracked};
pub use reactive_graph::wrappers::read::ArcSignal;
pub use vello_cpu::kurbo;
