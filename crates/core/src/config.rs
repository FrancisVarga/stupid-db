use std::env;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Load .env file (silently ignores if missing).
pub fn load_dotenv() {
    dotenvy::dotenv().ok();
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_opt(key: &str) -> Option<String> {
    env::var(key).ok().filter(|s| !s.is_empty())
}

/// Read a profiled env var: tries {PROFILE}_{KEY} first, falls back to {KEY}.
fn profiled_env_opt(profile: &str, key: &str) -> Option<String> {
    if !profile.is_empty() {
        let prefixed = format!("{}_{}", profile, key);
        if let Some(v) = env_opt(&prefixed) {
            return Some(v);
        }
    }
    env_opt(key)
}

fn profiled_env_or(profile: &str, key: &str, default: &str) -> String {
    profiled_env_opt(profile, key).unwrap_or_else(|| default.to_string())
}

fn profiled_env_u16(profile: &str, key: &str, default: u16) -> u16 {
    profiled_env_opt(profile, key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn profiled_env_u32(profile: &str, key: &str, default: u32) -> u32 {
    profiled_env_opt(profile, key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

// ── Top-level config ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Active profile name (empty = default).
    pub profile: String,
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub aws: AwsConfig,
    pub postgres: PostgresConfig,
    pub opensearch: OpenSearchConfig,
    pub llm: LlmConfig,
    pub ollama: OllamaConfig,
    pub embedding: EmbeddingConfig,
}

/// Well-known env keys that identify a profile when prefixed.
const PROFILE_MARKER_KEYS: &[&str] = &[
    "AWS_ACCESS_KEY_ID",
    "PG_HOST",
    "OPENSEARCH_HOST",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "S3_BUCKET",
];

impl Config {
    /// Build config from environment variables (call `load_dotenv()` first).
    /// Profile is read from `STUPID_PROFILE` env var. When set (e.g. `PROD`),
    /// every key is first looked up as `{PROFILE}_{KEY}`, falling back to `{KEY}`.
    pub fn from_env() -> Self {
        let profile = env_or("STUPID_PROFILE", "").to_uppercase();
        Self::for_profile(&profile)
    }

    /// Build config for a specific named profile (empty string = default).
    pub fn for_profile(profile: &str) -> Self {
        let p = profile.to_uppercase();
        let p = p.as_str();
        Self {
            profile: p.to_string(),
            server: ServerConfig::from_env_profiled(p),
            storage: StorageConfig::from_env_profiled(p),
            aws: AwsConfig::from_env_profiled(p),
            postgres: PostgresConfig::from_env_profiled(p),
            opensearch: OpenSearchConfig::from_env_profiled(p),
            llm: LlmConfig::from_env_profiled(p),
            ollama: OllamaConfig::from_env_profiled(p),
            embedding: EmbeddingConfig::from_env_profiled(p),
        }
    }

    /// Discover available profiles by scanning env vars for `{PREFIX}_{MARKER_KEY}` patterns.
    /// Always includes "default" (the unprefixed config).
    pub fn available_profiles() -> Vec<String> {
        let mut profiles = std::collections::BTreeSet::new();
        profiles.insert("default".to_string());

        for (key, _) in env::vars() {
            for marker in PROFILE_MARKER_KEYS {
                if let Some(prefix) = key.strip_suffix(&format!("_{}", marker)) {
                    if !prefix.is_empty()
                        && prefix.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                    {
                        profiles.insert(prefix.to_string());
                    }
                }
            }
        }

        profiles.into_iter().collect()
    }

    pub fn profile_label(&self) -> &str {
        if self.profile.is_empty() { "default" } else { &self.profile }
    }

    /// Print a redacted summary for startup logs.
    pub fn log_summary(&self) {
        tracing::info!("Config loaded (profile: {}):", self.profile_label());
        tracing::info!("  server:      port={}", self.server.port);
        tracing::info!("  storage:     data_dir={}", self.storage.data_dir.display());
        tracing::info!("  aws:         region={}, bucket={}", self.aws.region, self.aws.s3_bucket.as_deref().unwrap_or("(none)"));
        tracing::info!("  postgres:    host={}, db={}", self.postgres.host, self.postgres.database);
        tracing::info!("  opensearch:  host={}, index={}", self.opensearch.host, self.opensearch.index);
        tracing::info!("  llm:         provider={}", self.llm.provider);
        tracing::info!("  ollama:      url={}", self.ollama.url);
        tracing::info!("  embedding:   provider={}", self.embedding.provider);
    }

    /// Return a redacted view safe for API responses (no secrets).
    pub fn redacted_summary(&self) -> serde_json::Value {
        serde_json::json!({
            "profile": self.profile_label(),
            "server": { "host": self.server.host, "port": self.server.port },
            "storage": { "data_dir": self.storage.data_dir, "retention_days": self.storage.segment_retention_days },
            "aws": {
                "region": self.aws.region,
                "s3_bucket": self.aws.s3_bucket,
                "configured": self.aws.is_configured(),
            },
            "postgres": {
                "host": self.postgres.host,
                "port": self.postgres.port,
                "database": self.postgres.database,
                "configured": self.postgres.is_configured(),
            },
            "opensearch": {
                "host": self.opensearch.host,
                "port": self.opensearch.port,
                "index": self.opensearch.index,
                "configured": self.opensearch.is_configured(),
            },
            "llm": {
                "provider": self.llm.provider,
                "configured": self.llm.is_configured(),
            },
            "ollama": { "url": self.ollama.url, "model": self.ollama.model },
            "embedding": { "provider": self.embedding.provider, "dimensions": self.embedding.dimensions },
        })
    }
}

// ── Server ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub cors_origin: String,
}

impl ServerConfig {
    fn from_env_profiled(p: &str) -> Self {
        Self {
            host: profiled_env_or(p, "HOST", "0.0.0.0"),
            port: profiled_env_u16(p, "PORT", 3001),
            cors_origin: profiled_env_or(p, "CORS_ORIGIN", "*"),
        }
    }
}

// ── Storage ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub data_dir: PathBuf,
    pub segment_retention_days: u32,
    pub cache_dir: PathBuf,
    pub cache_max_gb: u32,
}

impl StorageConfig {
    fn from_env_profiled(p: &str) -> Self {
        let data_dir = PathBuf::from(profiled_env_or(p, "DATA_DIR", "data"));
        let cache_dir = PathBuf::from(
            profiled_env_or(p, "S3_CACHE_DIR", data_dir.join("cache").to_str().unwrap_or("data/cache")),
        );
        Self {
            data_dir,
            segment_retention_days: profiled_env_u32(p, "SEGMENT_RETENTION_DAYS", 30),
            cache_dir,
            cache_max_gb: profiled_env_u32(p, "S3_CACHE_MAX_GB", 50),
        }
    }
}

// ── AWS / S3 ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwsConfig {
    pub region: String,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub session_token: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_prefix: Option<String>,
    pub endpoint_url: Option<String>,
}

impl AwsConfig {
    fn from_env_profiled(p: &str) -> Self {
        Self {
            region: profiled_env_or(p, "AWS_REGION", "ap-southeast-1"),
            access_key_id: profiled_env_opt(p, "AWS_ACCESS_KEY_ID"),
            secret_access_key: profiled_env_opt(p, "AWS_SECRET_ACCESS_KEY"),
            session_token: profiled_env_opt(p, "AWS_SESSION_TOKEN"),
            s3_bucket: profiled_env_opt(p, "S3_BUCKET"),
            s3_prefix: profiled_env_opt(p, "S3_PREFIX"),
            endpoint_url: profiled_env_opt(p, "AWS_ENDPOINT_URL"),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.access_key_id.is_some() && self.s3_bucket.is_some()
    }
}

// ── PostgreSQL ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub ssl_mode: String,
    pub max_connections: u32,
}

impl PostgresConfig {
    fn from_env_profiled(p: &str) -> Self {
        Self {
            host: profiled_env_or(p, "PG_HOST", "localhost"),
            port: profiled_env_u16(p, "PG_PORT", 5432),
            database: profiled_env_or(p, "PG_DATABASE", "stupiddb"),
            username: profiled_env_opt(p, "PG_USERNAME"),
            password: profiled_env_opt(p, "PG_PASSWORD"),
            ssl_mode: profiled_env_or(p, "PG_SSL_MODE", "prefer"),
            max_connections: profiled_env_u32(p, "PG_MAX_CONNECTIONS", 10),
        }
    }

    pub fn connection_string(&self) -> String {
        let user = self.username.as_deref().unwrap_or("postgres");
        let pass = self.password.as_deref().unwrap_or("");
        format!(
            "postgres://{}:{}@{}:{}/{}?sslmode={}",
            user, pass, self.host, self.port, self.database, self.ssl_mode
        )
    }

    pub fn is_configured(&self) -> bool {
        self.username.is_some()
    }
}

// ── OpenSearch / Elasticsearch ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSearchConfig {
    pub host: String,
    pub port: u16,
    pub index: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub use_ssl: bool,
}

impl OpenSearchConfig {
    fn from_env_profiled(p: &str) -> Self {
        Self {
            host: profiled_env_or(p, "OPENSEARCH_HOST", "localhost"),
            port: profiled_env_u16(p, "OPENSEARCH_PORT", 9200),
            index: profiled_env_or(p, "OPENSEARCH_INDEX", "events"),
            username: profiled_env_opt(p, "OPENSEARCH_USERNAME"),
            password: profiled_env_opt(p, "OPENSEARCH_PASSWORD"),
            use_ssl: profiled_env_or(p, "OPENSEARCH_USE_SSL", "false") == "true",
        }
    }

    pub fn base_url(&self) -> String {
        let scheme = if self.use_ssl { "https" } else { "http" };
        format!("{}://{}:{}", scheme, self.host, self.port)
    }

    pub fn is_configured(&self) -> bool {
        self.host != "localhost" || self.username.is_some()
    }
}

// ── LLM (OpenAI / Anthropic) ─────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// "openai", "anthropic", "ollama"
    pub provider: String,
    pub openai_api_key: Option<String>,
    pub openai_model: String,
    pub openai_base_url: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub anthropic_model: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl LlmConfig {
    fn from_env_profiled(p: &str) -> Self {
        Self {
            provider: profiled_env_or(p, "LLM_PROVIDER", "ollama"),
            openai_api_key: profiled_env_opt(p, "OPENAI_API_KEY"),
            openai_model: profiled_env_or(p, "OPENAI_MODEL", "gpt-4o"),
            openai_base_url: profiled_env_opt(p, "OPENAI_BASE_URL"),
            anthropic_api_key: profiled_env_opt(p, "ANTHROPIC_API_KEY"),
            anthropic_model: profiled_env_or(p, "ANTHROPIC_MODEL", "claude-sonnet-4-5-20250929"),
            temperature: profiled_env_or(p, "LLM_TEMPERATURE", "0.1")
                .parse()
                .unwrap_or(0.1),
            max_tokens: profiled_env_u32(p, "LLM_MAX_TOKENS", 4096),
        }
    }

    pub fn is_configured(&self) -> bool {
        match self.provider.as_str() {
            "openai" => self.openai_api_key.is_some(),
            "anthropic" => self.anthropic_api_key.is_some(),
            "ollama" => true,
            _ => false,
        }
    }
}

// ── Ollama (local models) ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub url: String,
    pub model: String,
    pub embedding_model: String,
}

impl OllamaConfig {
    fn from_env_profiled(p: &str) -> Self {
        Self {
            url: profiled_env_or(p, "OLLAMA_URL", "http://localhost:11434"),
            model: profiled_env_or(p, "OLLAMA_MODEL", "llama3.2"),
            embedding_model: profiled_env_or(p, "OLLAMA_EMBEDDING_MODEL", "nomic-embed-text"),
        }
    }
}

// ── Embedding ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// "onnx", "ollama", "openai"
    pub provider: String,
    pub dimensions: u32,
    pub onnx_model_path: Option<String>,
    pub batch_size: u32,
}

impl EmbeddingConfig {
    fn from_env_profiled(p: &str) -> Self {
        Self {
            provider: profiled_env_or(p, "EMBEDDING_PROVIDER", "ollama"),
            dimensions: profiled_env_u32(p, "EMBEDDING_DIMENSIONS", 768),
            onnx_model_path: profiled_env_opt(p, "ONNX_MODEL_PATH"),
            batch_size: profiled_env_u32(p, "EMBEDDING_BATCH_SIZE", 64),
        }
    }
}
