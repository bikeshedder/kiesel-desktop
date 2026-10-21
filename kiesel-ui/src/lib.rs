//! Kiesel UI as a native Rust toolkit: Soft UI drawn by vello_cpu in a winit window.

mod a11y;
pub mod anim;
pub mod button;
pub mod paint;
pub mod row;
pub mod shell;
pub mod text;
pub mod theme;
pub mod widget;

pub use button::Button;
pub use row::Row;
pub use shell::{Options, run};
pub use theme::{Appearance, Scheme, Theme};
pub use widget::{Event, Widget, WidgetId};

pub use reactive_graph::signal::ArcRwSignal;
pub use reactive_graph::traits::{GetUntracked, Set, Update};
pub use vello_cpu::kurbo;
