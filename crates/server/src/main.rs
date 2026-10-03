use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;
use tendly_server::config::Config;

#[derive(Parser)]
#[command(name = "tendly", version, about = "Tendly — calm life admin. Self-hosted server, worker and tools.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the web server (and, unless TENDLY_EMBEDDED_WORKER=false, the background worker).
    Serve,
    /// Run the connector / calendar worker on its own.
    Worker {
        /// Process due work once and exit (for cron or a systemd timer).
        #[arg(long)]
        once: bool,
        #[arg(long, default_value_t = 60)]
        interval_secs: u64,
    },
    /// Write a consistent copy of the database to a new file.
    Backup {
        #[arg(long)]
        out: PathBuf,
    },
    /// Replace the database with a backup. Stop the server first.
    Restore {
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        yes: bool,
    },
    /// Export all data (without secrets) as JSON.
    Export {
        #[arg(long)]
        out: PathBuf,
    },
    /// Fill an empty database with synthetic demo data.
    SeedDemo,
    /// Manage paired devices for remote mode.
    Device {
        #[command(subcommand)]
        action: DeviceCmd,
    },
    /// Print a new random encryption key for TENDLY_ENCRYPTION_KEY.
    GenKey,
    /// Validate configuration and exit.
    CheckConfig,
}

#[derive(Subcommand)]
enum DeviceCmd {
    /// Create a pairing code (shown once).
    Add {
        #[arg(long)]
        name: String,
    },
    List,
    Revoke {
        id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("TENDLY_LOG").unwrap_or_else(|_| "info,sqlx=warn,tower_http=warn".into()))
        .with_target(false)
        .init();
    let cli = Cli::parse();
    match cli.command {
        Command::GenKey => {
            use base64::Engine;
            use rand::RngCore;
            let mut k = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut k);
            println!("{}", base64::engine::general_purpose::STANDARD.encode(k));
            return Ok(());
        }
        Command::CheckConfig => {
            let c = Config::from_env()?;
            println!("Configuration OK: mode={:?}, bind={}, data_dir={}", c.mode, c.bind, c.data_dir.display());
            return Ok(());
        }
        Command::Restore { from, yes } => {
            if !yes {
                bail!("Restoring replaces the current database. Stop the server, then re-run with --yes.");
            }
            let c = Config::from_env()?;
            tendly_server::backup::restore_from(&from, &c.database_path).await?;
            println!("Restored. The previous database was kept next to it with the suffix .before-restore.");
            return Ok(());
        }
        _ => {}
    }
    let config = Config::from_env()?;
    let state = tendly_server::init_state(config).await?;
    match cli.command {
        Command::Serve => {
            if state.config.embedded_worker {
                tokio::spawn(tendly_server::worker::run_forever(state.clone(), std::time::Duration::from_secs(60)));
            }
            let addr = state.config.bind;
            let app = tendly_server::router(state.clone());
            let listener = tokio::net::TcpListener::bind(addr).await?;
            tracing::info!("Tendly is listening on http://{} (mode: {:?})", listener.local_addr()?, state.config.mode);
            axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
        }
        Command::Worker { once, interval_secs } => {
            if once {
                let n = tendly_server::worker::run_once(&state, 200).await?;
                println!("Processed {n} job(s).");
            } else {
                tendly_server::worker::run_forever(state, std::time::Duration::from_secs(interval_secs.max(10))).await;
            }
        }
        Command::Backup { out } => {
            tendly_server::backup::backup_to(&state, &out).await?;
            println!("Backup written to {}", out.display());
        }
        Command::Export { out } => {
            let v = tendly_server::backup::export_json(&state).await?;
            std::fs::write(&out, serde_json::to_string_pretty(&v)?)?;
            println!("Export written to {} (secrets excluded).", out.display());
        }
        Command::SeedDemo => {
            if tendly_server::seed::seed_demo(&state).await? {
                println!("Demo data added.");
            } else {
                println!("The database already has people in it; demo data was not added.");
            }
        }
        Command::Device { action } => match action {
            DeviceCmd::Add { name } => {
                let (id, code) = tendly_server::routes::admin::create_device_token(&state, &name).await?;
                println!("Device {id} created. Pairing code (shown once):\n{code}");
            }
            DeviceCmd::List => {
                let rows: Vec<(String, String, Option<String>, Option<String>)> =
                    sqlx::query_as("SELECT id, name, last_seen_at, revoked_at FROM devices ORDER BY created_at").fetch_all(&state.db).await?;
                for (id, name, seen, revoked) in rows {
                    println!("{id}\t{name}\tlast seen: {}\t{}", seen.unwrap_or_else(|| "never".into()), if revoked.is_some() { "REVOKED" } else { "active" });
                }
            }
            DeviceCmd::Revoke { id } => {
                sqlx::query("UPDATE devices SET revoked_at = datetime('now') WHERE id = ?").bind(&id).execute(&state.db).await?;
                println!("Revoked {id}.");
            }
        },
        _ => unreachable!(),
    }
    Ok(())
}
