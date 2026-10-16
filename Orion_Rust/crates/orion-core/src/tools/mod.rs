//! Ferramentas nativas, todas sob a política de risco.

pub mod builtin;
pub mod desktop;
pub mod documents;
pub mod fs_tools;
pub mod ooxml;
pub mod ops_tools;
pub mod processes;
pub mod system_tools;
pub mod registry;

pub use registry::{Args, Tool, ToolError, ToolRegistry};
