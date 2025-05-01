use anyhow::{Context, Result};
use std::env;
use std::net::SocketAddr;
use chrono::Local;

/// Application configuration loaded from environment variables.
pub struct Config {
    /// Postgres connection URL
    pub database_url: String,
    /// API authorization key
    pub auth_key: String,
    /// Address and port to listen on
    pub listen_addr: SocketAddr,
    /// Timezone of the server (for timestamping)
    pub timezone: String,
}

impl Config {
    /// Load configuration from environment variables, with optional `.env` support.
    pub fn from_env() -> Result<Self> {
        // Load .env file if present
        dotenv::dotenv().ok();

        // Required variables
        let database_url = env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        let auth_key = env::var("AUTH_KEY").context("AUTH_KEY must be set")?;

        // Listen address, default to 0.0.0.0:2682
        let listen_addr_str = env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:2682".to_string());
        let listen_addr: SocketAddr = listen_addr_str
            .parse()
            .context("LISTEN_ADDR must be a valid socket address, e.g., 0.0.0.0:2682")?;

        // Timezone, default to local
        let timezone = env::var("TZ").unwrap_or_else(|_| {
            // Fallback to system local timezone offset
            Local::now().offset().to_string()
        });

        Ok(Config {
            database_url,
            auth_key,
            listen_addr,
            timezone,
        })
    }
}
