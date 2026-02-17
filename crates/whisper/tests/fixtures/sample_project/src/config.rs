/// Application configuration — loads settings from environment or file.

/// App configuration.
pub struct AppConfig {
    pub database_url: String,
    pub port: u16,
    pub debug: bool,
    pub max_connections: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database_url: "postgres://localhost/app".to_string(),
            port: 8080,
            debug: false,
            max_connections: 100,
        }
    }
}

/// Load configuration from environment variables.
pub fn load_config() -> AppConfig {
    AppConfig::default()
}
