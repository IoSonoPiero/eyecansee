//! The image editor (issue #53), without the interface: the document being edited (`doc`) and its
//! undo history (`history`), the objects under the mouse (`geom`), the objects drawn (`render`),
//! the copy saved (`save`). The image itself is changed only when saving.

pub mod doc;
pub mod geom;
pub mod history;
pub mod render;
// ponytail: the copy is written from the next step of the editor (issue #53, step 6 of notes/editor-piano.md).
#[cfg_attr(not(test), allow(dead_code))]
pub mod save;
