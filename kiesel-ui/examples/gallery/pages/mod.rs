//! The gallery's pages, one module each: `pub fn page(app: &App) -> Flex`

pub mod button;
pub mod choices;
pub mod dialog;
pub mod field;
pub mod form;
pub mod i18n;
pub mod icons;
pub mod inputs;
pub mod list;
pub mod popover;
pub mod segmented;
pub mod shortcuts;
pub mod textarea;
pub mod theme;
pub mod todo;
pub mod variants;

/// The pages' own messages, added once at start
pub fn catalogs() {
    i18n::catalogs();
    shortcuts::catalogs();
}
