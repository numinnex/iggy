mod args;
mod binary;
mod components;
mod http;
mod quic;
mod server_command;
mod server_config;
mod server_error;
mod tcp;

use crate::args::Args;
use crate::components::{channel, config_provider, message_cleaner, message_saver};
use crate::http::http_server;
use crate::quic::quic_server;
use crate::server_command::ServerCommand;
use crate::server_config::ServerConfig;
use crate::server_error::ServerError;
use crate::tcp::tcp_server;
use anyhow::Result;
use clap::Parser;
use figlet_rs::FIGfont;
use std::sync::Arc;
use streaming::persister::FileWithSyncPersister;
use streaming::segments::storage::FileSegmentStorage;
use streaming::systems::system::System;
use tokio::sync::RwLock;
use tokio::time::Instant;
use tracing::info;
#[tokio::main]
async fn main() -> Result<(), ServerError> {
    let args = Args::parse();
    let (non_blocking, _guard) = tracing_appender::non_blocking(std::io::stdout());
    let subscriber = tracing_subscriber::fmt().with_writer(non_blocking).finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Unable to set global default tracing subscriber");
    let standard_font = FIGfont::standard().unwrap();
    let figure = standard_font.convert("Iggy Server");
    println!("{}", figure.unwrap());
    info!(
        "Version: {}, hash: {}, built at: {} using rust version: {} for target: {}",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_DESCRIBE"),
        env!("VERGEN_BUILD_TIMESTAMP"),
        env!("VERGEN_RUSTC_SEMVER"),
        env!("VERGEN_CARGO_TARGET_TRIPLE")
    );
    let config_provider = config_provider::resolve(&args.config_provider)?;
    let config = ServerConfig::load(config_provider.as_ref()).await?;
    let mut system = System::new(config.system.clone(), None);
    system.init().await?;
    let system = Arc::new(RwLock::new(system));
    let (sender, receiver) = flume::unbounded::<ServerCommand>();
    message_cleaner::start(config.message_cleaner, system.clone());
    message_saver::start(config.message_saver, sender.clone());
    channel::start(system.clone(), receiver);

    #[cfg(unix)]
    let (mut ctrl_c, mut sigterm) = {
        use tokio::signal::unix::{signal, SignalKind};
        (
            signal(SignalKind::interrupt())?,
            signal(SignalKind::terminate())?,
        )
    };

    #[cfg(windows)]
    let mut ctrl_c = tokio::signal::ctrl_c();

    if config.http.enabled {
        let system = system.clone();
        tokio::spawn(async move {
            http_server::start(config.http, system).await;
        });
    }

    if config.quic.enabled {
        quic_server::start(config.quic, system.clone());
    }

    if config.tcp.enabled {
        tcp_server::start(config.tcp, system.clone());
    }

    #[cfg(unix)]
    tokio::select! {
        _ = ctrl_c.recv() => {
            info!("Received SIGINT. Shutting down Iggy server...");
        },
        _ = sigterm.recv() => {
            info!("Received SIGTERM. Shutting down Iggy server...");
        }
    }

    #[cfg(windows)]
    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            info!("Received CTRL-C. Shutting down Iggy server...");
        }
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
        }
    }

    let start_time = Instant::now();
    let mut system = system.write().await;
    let persister = Arc::new(FileWithSyncPersister);
    let storage = Arc::new(FileSegmentStorage::new(persister));
    system.shutdown(storage).await?;
    let elapsed_time = start_time.elapsed();

    info!(
        "Iggy server has shutdown successfully. Shutdown took {} ms.",
        elapsed_time.as_millis()
    );
    Ok(())
}
