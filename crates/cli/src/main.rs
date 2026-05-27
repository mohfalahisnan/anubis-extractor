use std::path::PathBuf;

use anubis_extractor::{
    cache::resolve_cache_dir, Config, ExtractOptions, Extractor, OcrOptions,
    TranscribeOptions, WhisperModel,
};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "anubis-extractor", version)]
struct Cli {
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,
    #[arg(long, global = true, default_value = "medium")]
    whisper_model: ModelArg,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    Transcribe {
        path: PathBuf,
        #[arg(long)]
        language: Option<String>,
        #[arg(long)]
        write_sidecar: bool,
        #[arg(long)]
        force: bool,
    },
    Ocr {
        path: PathBuf,
        #[arg(long)]
        write_sidecar: bool,
        #[arg(long)]
        force: bool,
    },
    Extract {
        path: PathBuf,
        #[arg(long)]
        language: Option<String>,
        #[arg(long)]
        write_sidecar: bool,
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum ModelArg {
    Tiny,
    Base,
    Small,
    Medium,
    LargeV3,
}
impl From<ModelArg> for WhisperModel {
    fn from(v: ModelArg) -> Self {
        match v {
            ModelArg::Tiny => Self::Tiny,
            ModelArg::Base => Self::Base,
            ModelArg::Small => Self::Small,
            ModelArg::Medium => Self::Medium,
            ModelArg::LargeV3 => Self::LargeV3,
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
    let extractor = Extractor::new(Config {
        cache_dir: resolve_cache_dir(cli.cache_dir),
        whisper_model: cli.whisper_model.into(),
    })?;
    match cli.cmd {
        Cmd::Transcribe { path, language, write_sidecar, force } => {
            let r = extractor.transcribe(&path, TranscribeOptions {
                language, model: None, write_sidecar, force, progress: None,
            }).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        Cmd::Ocr { path, write_sidecar, force } => {
            let r = extractor.ocr(&path, OcrOptions {
                write_sidecar, force, progress: None,
            }).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        Cmd::Extract { path, language, write_sidecar, force } => {
            let r = extractor.extract_text(&path, ExtractOptions {
                language, model: None, write_sidecar, force, progress: None,
            }).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
    }
    Ok(())
}
