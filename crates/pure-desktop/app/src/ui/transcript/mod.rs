//! The transcript's row model: the pure logic that turns one
//! session's transcript items into the rows the transcript renders,
//! plus the view state those rows are read with.
//!
//! [`RowModel`] is built once per session by [`row_model`]: it walks
//! `session.items` and folds consecutive same-family tool calls into
//! one [`Row::Group`], drops the turn boundaries tool calls follow,
//! and appends the changes run and the working row where the design
//! puts them. The model carries no measurement, no element ids and no
//! colors; the renderer owns those and reads what it needs from here.
//!
//! [`RowKey`] is the stable identity of a row, so the expansion
//! choices in [`UiState`] and the matches in [`FindMarks`] stay with
//! their row across rebuilds.

// The measured-height list's consumer, the transcript view, returns
// with S04.1; until then the engine is carried but unreferenced, and
// CI denies dead code.
pub(crate) mod head;
#[allow(dead_code)]
pub mod list;
pub mod markdown;
pub mod media_rows;
pub mod model;
pub(crate) mod prose;
pub mod text_rows;
pub mod tool_rows;
pub mod view;
