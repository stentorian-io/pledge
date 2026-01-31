//! Server configuration for the interlint API.

/// Server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Port to listen on.
    pub port: u16,
    /// Host address to bind to.
    pub host: [u8; 4],
}

impl Config {
    /// Create a new configuration from environment variables.
    ///
    /// # Environment Variables
    /// - `PORT`: Server port (default: 3000)
    /// - `HOST`: Host address (default: 0.0.0.0)
    pub fn from_env() -> Self {
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3000);

        let host = std::env::var("HOST")
            .ok()
            .and_then(|h| parse_host(&h))
            .unwrap_or([0, 0, 0, 0]);

        Self { port, host }
    }

    /// Get the socket address for binding.
    pub fn socket_addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::from((self.host, self.port))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 3000,
            host: [0, 0, 0, 0],
        }
    }
}

/// Parse a host string like "0.0.0.0" into a byte array.
fn parse_host(host: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return None;
    }

    let bytes: Result<Vec<u8>, _> = parts.iter().map(|p| p.parse::<u8>()).collect();
    bytes.ok().map(|b| [b[0], b[1], b[2], b[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.port, 3000);
        assert_eq!(config.host, [0, 0, 0, 0]);
    }

    #[test]
    fn test_parse_host() {
        assert_eq!(parse_host("0.0.0.0"), Some([0, 0, 0, 0]));
        assert_eq!(parse_host("127.0.0.1"), Some([127, 0, 0, 1]));
        assert_eq!(parse_host("192.168.1.1"), Some([192, 168, 1, 1]));
        assert_eq!(parse_host("invalid"), None);
        assert_eq!(parse_host("1.2.3"), None);
    }

    #[test]
    fn test_socket_addr() {
        let config = Config {
            port: 8080,
            host: [127, 0, 0, 1],
        };
        let addr = config.socket_addr();
        assert_eq!(addr.port(), 8080);
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
    }
}
