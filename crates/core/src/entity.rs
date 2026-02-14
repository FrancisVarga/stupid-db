use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type NodeId = Uuid;
pub type EdgeId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    Member,
    Device,
    Game,
    Affiliate,
    Currency,
    VipGroup,
    Error,
    Platform,
    Popup,
    Provider,
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityType::Member => write!(f, "Member"),
            EntityType::Device => write!(f, "Device"),
            EntityType::Game => write!(f, "Game"),
            EntityType::Affiliate => write!(f, "Affiliate"),
            EntityType::Currency => write!(f, "Currency"),
            EntityType::VipGroup => write!(f, "VipGroup"),
            EntityType::Error => write!(f, "Error"),
            EntityType::Platform => write!(f, "Platform"),
            EntityType::Popup => write!(f, "Popup"),
            EntityType::Provider => write!(f, "Provider"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeType {
    LoggedInFrom,
    OpenedGame,
    SawPopup,
    HitError,
    BelongsToGroup,
    ReferredBy,
    UsesCurrency,
    PlaysOnPlatform,
    ProvidedBy,
}

impl std::fmt::Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdgeType::LoggedInFrom => write!(f, "LoggedInFrom"),
            EdgeType::OpenedGame => write!(f, "OpenedGame"),
            EdgeType::SawPopup => write!(f, "SawPopup"),
            EdgeType::HitError => write!(f, "HitError"),
            EdgeType::BelongsToGroup => write!(f, "BelongsToGroup"),
            EdgeType::ReferredBy => write!(f, "ReferredBy"),
            EdgeType::UsesCurrency => write!(f, "UsesCurrency"),
            EdgeType::PlaysOnPlatform => write!(f, "PlaysOnPlatform"),
            EdgeType::ProvidedBy => write!(f, "ProvidedBy"),
        }
    }
}
