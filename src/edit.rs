//! The image editor (issue #53), without the interface: the document being edited (`doc`) and its
//! undo history (`history`), the objects under the mouse (`geom`), the objects drawn (`render`), what they take from the photo (`photo`),
//! the copy saved (`save`), the size changed (`resize`). The image itself is changed only when saving.

pub mod animated;
pub mod bubble;
pub mod doc;
pub mod filter;
pub mod geom;
pub mod history;
pub mod photo;
pub mod render;
pub mod resize;
pub mod save;
