//! Client side of plugin MCP servers: the `McpTransport` seam, its stdio and
//! streamable HTTP implementations (built on the `rmcp` client), the shared
//! server pool, and the gateway tool source over it.

mod client;
mod http;
pub mod naming;
mod pool;
mod source;
mod stdio;
mod transport;

pub use http::HttpTransport;
pub use pool::{McpServerPool, PoolConfig, PoolError, PoolServerSpec, ServerHealth, ServerKey};
pub use source::{DbPluginServerCatalog, PluginMcpSource, PluginServerCatalog, PluginServerNames};
pub use stdio::StdioTransport;
pub use transport::{McpConnection, McpTransport, ProxyError, RemoteTool, Transports};
