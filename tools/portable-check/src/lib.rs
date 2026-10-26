//! Compile the production modules, without replacing any Windows API with a stub.
#[path = "../../../src/activities/engine.rs"]
pub mod engine;
#[path = "../../../src/i18n.rs"]
pub mod i18n;
#[path = "../../../src/activities/timer.rs"]
pub mod timer;
