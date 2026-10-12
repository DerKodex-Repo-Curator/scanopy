//! A historical discovery's warnings, one row each, with every entity id they name held in a
//! foreign-key column. `Storable` rather than `Entity`: a warning is written once with its run and
//! read with it, and nothing creates, edits or deletes one on its own.
pub mod r#impl;
pub mod service;
