//! Client side of plugin MCP servers: the `McpTransport` seam and its stdio and
//! streamable HTTP implementations (built on the `rmcp` client).

mod client;
mod http;
mod stdio;
mod transport;

pub use http::HttpTransport;
pub use stdio::StdioTransport;
pub use transport::{McpConnection, McpTransport, ProxyError, RemoteTool, Transports};
