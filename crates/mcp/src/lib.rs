//! FigCraft's MCP server.
//!
//! [Model Context Protocol](https://modelcontextprotocol.io) over stdio: newline-delimited JSON-RPC
//! 2.0, hand-written (no async runtime). The server exposes FigCraft as MCP tools, resources and
//! prompts, and forwards everything to a [`Backend`]:
//!
//! - `Headless` (with the default feature) hosts an in-process engine session and answers the control-channel
//!   methods itself (rendering with `figcraft-render`), so agents can design without a window.
//! - [`Attached`] talks to a running desktop app through its loopback JSON-lines control channel
//!   (`figcraft --control PORT`): one `{"id","method","params"}` line in, one
//!   `{"id","ok","result"|"error"}` line out.
//!
//! Entry points: [`Server::serve`] (stdio loop) and [`Server::handle_line`] (one message).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod backend;
#[cfg(feature = "headless")]
mod headless;
mod prompts;
mod resources;
mod server;
mod tools;

#[cfg(test)]
#[cfg(feature = "headless")]
mod gradient_tests;

pub use backend::{Attached, Backend, control_token_path, decode_data, page_children};
#[cfg(feature = "headless")]
pub use headless::Headless;
pub use prompts::{PROMPTS, PromptArg, PromptDef};
pub use resources::{COMMAND_TEMPLATE, COMMANDS_URI, DOC_URI, NODE_TEMPLATE};
pub use server::{INSTRUCTIONS, PROTOCOL_VERSION, SUPPORTED_VERSIONS, Server};
pub use tools::{TOOL_NAMES, ToolResult, call_tool, tool_definitions};

/// Width and height of a PNG from its IHDR chunk.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..8)? != b"\x89PNG\r\n\x1a\n" || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let h = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    Some((w, h))
}
