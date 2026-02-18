use serde::{Deserialize, Serialize};

/// Transport layer for ZeroMQ connections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "address")]
pub enum Transport {
    /// Inter-process communication via Unix domain sockets.
    /// Fastest option for same-host communication.
    Ipc(String),

    /// TCP transport for distributed deployment.
    Tcp { host: String, port: u16 },
}

impl Transport {
    /// Create an IPC transport with the given socket name.
    ///
    /// The name is used as a path component under `/tmp/stupid-db/`.
    pub fn ipc(name: &str) -> Self {
        Self::Ipc(name.to_string())
    }

    /// Create a TCP transport with the given host and port.
    pub fn tcp(host: impl Into<String>, port: u16) -> Self {
        Self::Tcp {
            host: host.into(),
            port,
        }
    }

    /// Generate the ZeroMQ endpoint address string.
    pub fn endpoint(&self) -> String {
        match self {
            Self::Ipc(name) => format!("ipc:///tmp/stupid-db/{name}.sock"),
            Self::Tcp { host, port } => format!("tcp://{host}:{port}"),
        }
    }
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.endpoint())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_endpoint() {
        let t = Transport::ipc("broker");
        assert_eq!(t.endpoint(), "ipc:///tmp/stupid-db/broker.sock");
    }

    #[test]
    fn tcp_endpoint() {
        let t = Transport::tcp("127.0.0.1", 5555);
        assert_eq!(t.endpoint(), "tcp://127.0.0.1:5555");
    }

    #[test]
    fn display_matches_endpoint() {
        let t = Transport::tcp("localhost", 9090);
        assert_eq!(t.to_string(), t.endpoint());
    }
}
