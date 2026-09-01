//! Domain model, configuration and persistence for Beacon.
//!
//! This crate has no dependency on Tauri, on a window, or on a UI framework.
//! That is a deliberate constraint: session and process management will
//! eventually run in a background daemon so that closing the Beacon window does
//! not kill live Claude sessions, and everything here is meant to move there
//! unchanged.

pub mod agents;
pub mod appearance;
pub mod claude;
pub mod claude_hooks;
pub mod client;
pub mod clips;
pub mod detect;
pub mod domain;
pub mod dotenv;
pub mod error;
pub mod files;
pub mod git;
pub mod keymap;
pub mod layout;
pub mod paths;
pub mod protocol;
pub mod releases;
pub mod requirements;
pub mod scrollback;
pub mod session;
pub mod settings;
pub mod state;
pub mod store;
pub mod tools;
pub mod ui_state;
pub mod workstreams;

pub use error::{CoreError, Result};
pub use layout::{LayoutNode, LayoutPreset, PanelId, SplitDirection};
pub use session::{SessionEvents, SessionId, SessionInfo, SessionKind, SessionManager};
pub use state::{Beacon, ProjectView, Snapshot, WorkspaceView};
