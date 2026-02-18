use thiserror::Error;

/// Errors that can occur in the eisenbahn messaging layer.
#[derive(Debug, Error)]
pub enum EisenbahnError {
    #[error("serialization error: {0}")]
    Serialization(#[from] rmp_serde::encode::Error),

    #[error("deserialization error: {0}")]
    Deserialization(#[from] rmp_serde::decode::Error),

    #[error("zeromq error: {0}")]
    Zmq(#[from] zeromq::ZmqError),

    #[error("transport error: {0}")]
    Transport(String),

    #[error("connection timeout after {0:?}")]
    Timeout(std::time::Duration),

    #[error("config error: {0}")]
    Config(String),

    #[error("config parse error: {0}")]
    ConfigParse(#[from] toml::de::Error),

    #[error("circular pipeline dependency: {0}")]
    CircularDependency(String),

    #[error("config I/O error: {0}")]
    ConfigIo(#[from] std::io::Error),
}
