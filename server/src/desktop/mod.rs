//! `coppice-server desktop`: data dir layout, bundled resources, secrets and
//! generated config for the single-user desktop app.

pub mod bootstrap;
pub mod layout;
pub mod postgres;

/// A loopback port that was free a moment ago; the caller binds it next.
pub fn free_loopback_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_loopback_port_is_bindable() {
        let port = free_loopback_port().expect("port");
        assert_ne!(port, 0);
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("bind free port");
    }
}
