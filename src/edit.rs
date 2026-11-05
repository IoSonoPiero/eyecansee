//! The image editor (issue #53), without the interface: the document being edited (`doc`) and its
//! undo history (`history`), the objects under the mouse (`geom`), the objects drawn (`render`),
//! the copy saved (`save`). The image itself is changed only when saving.

pub mod doc;
pub mod geom;
pub mod history;
pub mod render;
pub mod save;
