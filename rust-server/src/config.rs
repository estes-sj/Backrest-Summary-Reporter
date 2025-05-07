use std::{env, net::SocketAddr};

use anyhow::{Context, Result};
use chrono::Local;
use dotenv::dotenv;

/// Application configuration loaded from environment variables.
#[derive(Clone)]
pub struct Config {
    /// Postgres connection URL
    pub database_url: String,
    /// API authorization key
    pub auth_key: String,
    /// Address and port to listen on
    pub listen_addr: SocketAddr,
    /// Timezone of the server (for timestamping)
    pub timezone: String,

    // --- SMTP settings (optional) ---
    pub smtp_host: Option<String>,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub email_from: Option<String>,
    pub email_to: Option<String>,
}

impl Config {
    /// Load configuration from environment variables, with optional `.env` support.
    pub fn from_env() -> Result<Self> {
        // Load .env file if present
        dotenv().ok();

        // Required variables
        let database_url = env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
        let auth_key = env::var("AUTH_KEY").context("AUTH_KEY must be set")?;

        // Listen address, default to 0.0.0.0:2682
        let listen_addr_str =
            env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:2682".into());
        let listen_addr: SocketAddr = listen_addr_str
            .parse()
            .context("LISTEN_ADDR must be a valid socket address")?;

        // Timezone, default to local
        let timezone = env::var("TZ").unwrap_or_else(|_| Local::now().offset().to_string());

        // Optional SMTP / email settings
        let smtp_host = env::var("SMTP_HOST").ok();
        let smtp_username = env::var("SMTP_USERNAME").ok();
        let smtp_password = env::var("SMTP_PASSWORD").ok();
        let email_from = env::var("EMAIL_FROM").ok();
        let email_to = env::var("EMAIL_TO").ok();

        Ok(Config {
            database_url,
            auth_key,
            listen_addr,
            timezone,
            smtp_host,
            smtp_username,
            smtp_password,
            email_from,
            email_to,
        })
    }
}