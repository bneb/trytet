//! Model Context Protocol (MCP) server implementation.
//!
//! Exposes Trytet cartridges as MCP tools, agent state as resources,
//! and cartridge templates as prompts. Supports stdio and HTTP+SSE transport.

pub mod protocol;
pub mod server;
