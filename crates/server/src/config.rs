//! Runtime configuration from environment variables / CLI flags.
//!
//! Secrets are never read from the database or from the repository. They come
//! from environment variables, `*_FILE` variables pointing at protected files,
//! or (for the encryption key only) an auto-generated key file outside the
//! database.

use anyhow::{bail, Context, Result};
use ipnet::IpNet;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use tendly_core::api::ServerMode;

#[derive(Clone, Debug)]
pub struct Config {
    pub mode: ServerMode,
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    pub web_dir: Option<PathBuf>,
    pub allowed_hosts: Vec<String>,
    pub allowed_networks: Vec<IpNet>,
    pub trusted_fetch_networks: Vec<IpNet>,
    pub default_timezone: String,
    pub encryption_key: Option<String>,
    pub encryption_key_file: Option<PathBuf>,
    pub admin_token: Option<String>,
    pub cookie_secure: bool,
    pub embedded_worker: bool,
    pub demo: bool,
    pub fixture_dir: Option<PathBuf>,
    pub oauth: OAuthConfig,
    pub ai_env_key: Option<String>,
    pub provider_base_overrides: ProviderBases,
    /// Allow calendar URL fetches through the environment's HTTP proxy. Off by
    /// default because a proxy resolves names itself, bypassing IP pinning.
    pub fetch_via_proxy: bool,
}

#[derive(Clone, Debug, Default)]
pub struct OAuthConfig {
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub microsoft_client_id: Option<String>,
    pub microsoft_client_secret: Option<String>,
    pub microsoft_tenant: String,
    pub redirect_base: Option<String>,
}

/// Base URLs for provider APIs; overridable only for tests and private deployments.
#[derive(Clone, Debug)]
pub struct ProviderBases {
    pub gmail: String,
    pub google_token: String,
    pub google_auth: String,
    pub graph: String,
    pub microsoft_login: String,
    pub slack: String,
    pub anthropic: String,
}

impl Default for ProviderBases {
    fn default() -> Self {
        ProviderBases {
            gmail: "https://gmail.googleapis.com".into(),
            google_token: "https://oauth2.googleapis.com/token".into(),
            google_auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            graph: "https://graph.microsoft.com/v1.0".into(),
            microsoft_login: "https://login.microsoftonline.com".into(),
            slack: "https://slack.com/api".into(),
            anthropic: "https://api.anthropic.com".into(),
        }
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Reads `NAME` or the file named by `NAME_FILE` (trimmed).
pub fn secret_env(name: &str) -> Result<Option<String>> {
    if let Some(v) = env(name) {
        return Ok(Some(v));
    }
    if let Some(path) = env(&format!("{name}_FILE")) {
        let v = std::fs::read_to_string(&path).with_context(|| format!("reading {name}_FILE"))?;
        return Ok(Some(v.trim().to_string()));
    }
    Ok(None)
}

fn parse_list(v: Option<String>) -> Vec<String> {
    v.map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
        .unwrap_or_default()
}

fn parse_nets(v: Option<String>, name: &str) -> Result<Vec<IpNet>> {
    parse_list(v)
        .into_iter()
        .map(|s| s.parse::<IpNet>().with_context(|| format!("{name}: invalid network {s}")))
        .collect()
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let mode = match env("TENDLY_MODE").as_deref().unwrap_or("local") {
            "local" => ServerMode::Local,
            "lan" => ServerMode::Lan,
            "remote" => ServerMode::Remote,
            other => bail!("TENDLY_MODE must be local, lan or remote (got {other})"),
        };
        let bind: SocketAddr = env("TENDLY_BIND")
            .unwrap_or_else(|| "127.0.0.1:7878".into())
            .parse()
            .context("TENDLY_BIND must look like 127.0.0.1:7878")?;
        let data_dir = PathBuf::from(env("TENDLY_DATA_DIR").unwrap_or_else(|| "./data".into()));
        let database_path = env("TENDLY_DATABASE_PATH").map(PathBuf::from).unwrap_or_else(|| data_dir.join("tendly.db"));
        let cfg = Config {
            mode,
            bind,
            database_path,
            web_dir: env("TENDLY_WEB_DIR").map(PathBuf::from),
            allowed_hosts: parse_list(env("TENDLY_ALLOWED_HOSTS")),
            allowed_networks: parse_nets(env("TENDLY_ALLOWED_NETWORKS"), "TENDLY_ALLOWED_NETWORKS")?,
            trusted_fetch_networks: parse_nets(env("TENDLY_TRUSTED_FETCH_NETWORKS"), "TENDLY_TRUSTED_FETCH_NETWORKS")?,
            default_timezone: env("TENDLY_TIMEZONE").unwrap_or_else(|| "UTC".into()),
            encryption_key: secret_env("TENDLY_ENCRYPTION_KEY")?,
            encryption_key_file: env("TENDLY_ENCRYPTION_KEY_PATH").map(PathBuf::from),
            admin_token: secret_env("TENDLY_ADMIN_TOKEN")?,
            cookie_secure: env("TENDLY_COOKIE_SECURE").map(|v| v == "true").unwrap_or(mode == ServerMode::Remote),
            embedded_worker: env("TENDLY_EMBEDDED_WORKER").map(|v| v != "false").unwrap_or(true),
            demo: env("TENDLY_DEMO").map(|v| v == "true").unwrap_or(false),
            fixture_dir: env("TENDLY_FIXTURE_DIR").map(PathBuf::from),
            oauth: OAuthConfig {
                google_client_id: env("TENDLY_GOOGLE_CLIENT_ID"),
                google_client_secret: secret_env("TENDLY_GOOGLE_CLIENT_SECRET")?,
                microsoft_client_id: env("TENDLY_MICROSOFT_CLIENT_ID"),
                microsoft_client_secret: secret_env("TENDLY_MICROSOFT_CLIENT_SECRET")?,
                microsoft_tenant: env("TENDLY_MICROSOFT_TENANT").unwrap_or_else(|| "common".into()),
                redirect_base: env("TENDLY_OAUTH_REDIRECT_BASE"),
            },
            ai_env_key: secret_env("TENDLY_AI_API_KEY")?,
            provider_base_overrides: ProviderBases::default(),
            fetch_via_proxy: env("TENDLY_FETCH_VIA_PROXY").map(|v| v == "true").unwrap_or(false),
            data_dir,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// A configuration for tests and embedded use: loopback, temp data dir.
    pub fn for_data_dir(data_dir: PathBuf) -> Self {
        Config {
            mode: ServerMode::Local,
            bind: "127.0.0.1:0".parse().expect("valid"),
            database_path: data_dir.join("tendly.db"),
            web_dir: None,
            allowed_hosts: vec![],
            allowed_networks: vec![],
            trusted_fetch_networks: vec![],
            default_timezone: "UTC".into(),
            encryption_key: None,
            encryption_key_file: None,
            admin_token: None,
            cookie_secure: false,
            embedded_worker: false,
            demo: false,
            fixture_dir: None,
            oauth: OAuthConfig { microsoft_tenant: "common".into(), ..Default::default() },
            ai_env_key: None,
            provider_base_overrides: ProviderBases::default(),
            fetch_via_proxy: false,
            data_dir,
        }
    }

    pub fn validate(&self) -> Result<()> {
        let ip = self.bind.ip();
        match self.mode {
            ServerMode::Local => {
                if !ip.is_loopback() {
                    bail!("Local mode only binds to loopback (127.0.0.1 or ::1). Use TENDLY_MODE=lan or remote to expose it deliberately.");
                }
            }
            ServerMode::Lan => {
                if self.allowed_networks.is_empty() {
                    bail!("LAN mode requires TENDLY_ALLOWED_NETWORKS, e.g. 192.168.1.0/24.");
                }
                if let Some(bad) = self.allowed_networks.iter().find(|n| !is_private_net(n)) {
                    bail!("LAN mode only allows private networks; {bad} is public. Use remote mode with device pairing instead.");
                }
            }
            ServerMode::Remote => {
                if self.admin_token.as_deref().map(|t| t.len() < 24).unwrap_or(true) {
                    bail!("Remote mode requires TENDLY_ADMIN_TOKEN (or TENDLY_ADMIN_TOKEN_FILE) with at least 24 characters.");
                }
                if self.allowed_hosts.is_empty() {
                    bail!("Remote mode requires TENDLY_ALLOWED_HOSTS with the public host name, e.g. tendly.example.org.");
                }
            }
        }
        if self.default_timezone.parse::<chrono_tz::Tz>().is_err() {
            bail!("TENDLY_TIMEZONE must be an IANA zone such as Europe/Stockholm.");
        }
        Ok(())
    }

    /// Host header values accepted by the server (DNS-rebinding protection).
    pub fn effective_allowed_hosts(&self) -> Vec<String> {
        let mut hosts: Vec<String> = vec!["localhost".into(), "127.0.0.1".into(), "[::1]".into(), "tauri.localhost".into()];
        let ip = self.bind.ip();
        if !ip.is_unspecified() && !ip.is_loopback() {
            hosts.push(match ip {
                IpAddr::V6(v6) => format!("[{v6}]"),
                v4 => v4.to_string(),
            });
        }
        hosts.extend(self.allowed_hosts.iter().map(|h| h.to_ascii_lowercase()));
        hosts
    }
}

pub fn is_private_net(n: &IpNet) -> bool {
    match n.network() {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local() || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64),
        IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00 || (v6.segments()[0] & 0xffc0) == 0xfe80,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_mode_rejects_public_bind() {
        let mut c = Config::for_data_dir("/tmp/x".into());
        assert!(c.validate().is_ok());
        c.bind = "0.0.0.0:7878".parse().unwrap();
        assert!(c.validate().is_err());
    }

    #[test]
    fn lan_mode_requires_private_allowlist() {
        let mut c = Config::for_data_dir("/tmp/x".into());
        c.mode = ServerMode::Lan;
        c.bind = "0.0.0.0:7878".parse().unwrap();
        assert!(c.validate().is_err());
        c.allowed_networks = vec!["8.8.8.0/24".parse().unwrap()];
        assert!(c.validate().is_err());
        c.allowed_networks = vec!["192.168.1.0/24".parse().unwrap()];
        assert!(c.validate().is_ok());
    }

    #[test]
    fn remote_mode_requires_admin_token() {
        let mut c = Config::for_data_dir("/tmp/x".into());
        c.mode = ServerMode::Remote;
        assert!(c.validate().is_err());
        c.admin_token = Some("x".repeat(32));
        assert!(c.validate().is_err());
        c.allowed_hosts = vec!["tendly.example.org".into()];
        assert!(c.validate().is_ok());
    }
}
