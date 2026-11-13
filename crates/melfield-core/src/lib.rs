//! Melfield core. No Tauri dependency: shared by the desktop app, the local API and the CLI.

pub mod audio;
pub mod engines;
pub mod error;
pub mod g2p;
pub mod hardware;
pub mod paths;
pub mod pull;
pub mod registry;
pub mod service;
pub mod store;
pub mod text;

pub use error::{ErrorCode, FixAction, MelError, MelResult};

pub use service::{Config, CoreEvent, EventSink, Melfield, NullSink};
