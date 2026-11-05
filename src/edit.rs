//! The image editor (issue #53), without the interface: the document being edited (`doc`) and its
//! undo history (`history`), the objects drawn (`render`). The image itself is changed only when saving.

pub mod doc;
pub mod history;
pub mod render;
