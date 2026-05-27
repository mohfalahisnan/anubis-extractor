use std::path::PathBuf;

use anubis_extractor::{cache::resolve_cache_dir, Config, Extractor, WhisperModel};
use anubis_extractor_mcp::server::{serve_http, serve_stdio, ExtractorServer};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "anubis-extractor-mcp", version)]
struct Cli {
    /// Override the cache dir. Equivalent to `ANUBIS_EXTRACTOR_CACHE_DIR`.
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,
    /// Default whisper model.
    #[arg(long, global = true, default_value = "medium")]
    whisper_model: WhisperModelArg,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Speak MCP over stdio.
    Stdio,
    /// Serve MCP over HTTP/SSE.
    Serve {
        #[arg(long, default_value = "127.0.0.1:7878")]
        bind: String,
        /// Allow binding to non-localhost.
        #[arg(long)]
        allow_remote: bool,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum WhisperModelArg {
    Tiny,
    Base,
    Small,
    Medium,
    LargeV3,
}

impl From<WhisperModelArg> for WhisperModel {
    fn from(v: WhisperModelArg) -> Self {
        match v {
            WhisperModelArg::Tiny => Self::Tiny,
            WhisperModelArg::Base => Self::Base,
            WhisperModelArg::Small => Self::Small,
            WhisperModelArg::Medium => Self::Medium,
            WhisperModelArg::LargeV3 => Self::LargeV3,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let cache = resolve_cache_dir(cli.cache_dir);
    let extractor = Extractor::new(Config {
        cache_dir: cache,
        whisper_model: cli.whisper_model.into(),
    })?;
    let server = ExtractorServer::new(extractor);

    match cli.cmd {
        Cmd::Stdio => serve_stdio(server).await?,
        Cmd::Serve { bind, allow_remote } => {
            if !allow_remote && !bind.starts_with("127.0.0.1") && !bind.starts_with("localhost") {
                anyhow::bail!("refusing to bind to {bind} without --allow-remote");
            }
            serve_http(server, &bind).await?;
        }
    }
    Ok(())
}
