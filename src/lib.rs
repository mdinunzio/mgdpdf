//! Library crate exposing the PDF/edit/tools subsystems so they can be
//! exercised by integration tests, examples, and (eventually) other binaries.
//! The `mgdpdf` binary (in `src/main.rs`) consumes these modules directly.

pub mod app;
pub mod edit;
pub mod pdf;
pub mod signature;
pub mod tools;
pub mod ui;
