//! MCP server exposing three extractor tools over rmcp.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use anubis_extractor::{
    ExtractOptions, Extractor, OcrOptions, Progress, TranscribeOptions, WhisperModel,
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::Parameters},
    model::{
        CallToolResult, Content, Implementation, Meta, ProgressNotificationParam, ProtocolVersion,
        ServerCapabilities, ServerInfo,
    },
    service::{Peer, RoleServer},
    tool, tool_handler, tool_router, Error as McpError, ServerHandler, ServiceExt,
};

use crate::tools::{ExtractInput, OcrInput, ToolOutput, TranscribeInput};

#[derive(Clone)]
pub struct ExtractorServer {
    pub extractor: Arc<Extractor>,
    tool_router: ToolRouter<Self>,
}

impl ExtractorServer {
    pub fn new(extractor: Extractor) -> Self {
        Self {
            extractor: Arc::new(extractor),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl ExtractorServer {
    #[tool(
        name = "extractor_transcribe",
        description = "Transcribe an audio or video file to text using whisper.cpp."
    )]
    async fn extractor_transcribe(
        &self,
        Parameters(input): Parameters<TranscribeInput>,
        meta: Meta,
        peer: Peer<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let path = PathBuf::from(input.path);

        // If the client included a progressToken in _meta, we bridge our
        // internal `Progress` channel to MCP `notifications/progress`.
        // Without a token the spec says we MUST NOT emit progress; we still
        // run the transcription, just without notifications.
        let progress_token = meta.get_progress_token();
        let (progress_sender, pump_handle) = if let Some(token) = progress_token {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<Progress>(64);
            let peer = peer.clone();
            let handle = tokio::spawn(async move {
                let mut emitted: u32 = 0;
                while let Some(ev) = rx.recv().await {
                    emitted = emitted.saturating_add(1);
                    let (total, message) = match &ev {
                        Progress::Download { artifact, bytes, total } => (
                            total.and_then(|t| u32::try_from(t).ok()),
                            Some(format!("download {artifact}: {bytes} bytes")),
                        ),
                        Progress::Stage { stage, message } => {
                            (None, Some(format!("{stage}: {message}")))
                        }
                        Progress::Segment { index, total, text } => (
                            total.and_then(|t| u32::try_from(t).ok()),
                            Some(format!("segment {index}: {text}")),
                        ),
                    };
                    let _ = peer
                        .notify_progress(ProgressNotificationParam {
                            progress_token: token.clone(),
                            progress: emitted,
                            total,
                            message,
                        })
                        .await;
                }
            });
            (Some(tx), Some(handle))
        } else {
            (None, None)
        };

        let opts = TranscribeOptions {
            language: input.language,
            model: input.model.as_deref().and_then(parse_model),
            write_sidecar: input.write_sidecar,
            force: input.force,
            progress: progress_sender,
        };
        let result = self
            .extractor
            .transcribe(&path, opts)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Drain the pump: dropping the sender (inside `opts`) closed the channel,
        // so the pump task will exit naturally once it processes remaining events.
        if let Some(handle) = pump_handle {
            let _ = handle.await;
        }

        Ok(CallToolResult::success(vec![Content::json(
            ToolOutput::Transcribe(result),
        )?]))
    }

    #[tool(
        name = "extractor_ocr",
        description = "Extract text from an image file using the ocrs engine."
    )]
    async fn extractor_ocr(
        &self,
        Parameters(input): Parameters<OcrInput>,
    ) -> Result<CallToolResult, McpError> {
        let path = PathBuf::from(input.path);
        let opts = OcrOptions {
            write_sidecar: input.write_sidecar,
            force: input.force,
            progress: None,
        };
        let result = self
            .extractor
            .ocr(&path, opts)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::json(
            ToolOutput::Ocr(result),
        )?]))
    }

    #[tool(
        name = "extractor_extract_text",
        description = "Auto-dispatch text extraction based on file extension."
    )]
    async fn extractor_extract_text(
        &self,
        Parameters(input): Parameters<ExtractInput>,
    ) -> Result<CallToolResult, McpError> {
        let path = PathBuf::from(input.path);
        let opts = ExtractOptions {
            language: input.language,
            model: input.model.as_deref().and_then(parse_model),
            write_sidecar: input.write_sidecar,
            force: input.force,
            progress: None,
        };
        let result = self
            .extractor
            .extract_text(&path, opts)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let out = match result {
            anubis_extractor::ExtractResult::Transcribe(r) => ToolOutput::Transcribe(r),
            anubis_extractor::ExtractResult::Ocr(r) => ToolOutput::Ocr(r),
        };
        Ok(CallToolResult::success(vec![Content::json(out)?]))
    }
}

#[tool_handler]
impl ServerHandler for ExtractorServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "anubis-extractor".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "OCR and audio/video transcription. Three tools: extractor_transcribe, \
                 extractor_ocr, extractor_extract_text. All take an absolute file path."
                    .into(),
            ),
        }
    }
}

pub async fn serve_stdio_with_transport<R, W>(
    server: ExtractorServer,
    read: R,
    write: W,
) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let service = server.serve((read, write)).await?;
    service.waiting().await?;
    Ok(())
}

pub async fn serve_stdio(server: ExtractorServer) -> anyhow::Result<()> {
    serve_stdio_with_transport(server, tokio::io::stdin(), tokio::io::stdout()).await
}

pub async fn serve_http(server: ExtractorServer, bind: &str) -> anyhow::Result<()> {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };

    let service = StreamableHttpService::new(
        move || Ok::<ExtractorServer, std::io::Error>(server.clone()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    let app = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!("listening on http://{bind}/mcp");
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn parse_model(s: &str) -> Option<WhisperModel> {
    match s.to_ascii_lowercase().as_str() {
        "tiny" => Some(WhisperModel::Tiny),
        "base" => Some(WhisperModel::Base),
        "small" => Some(WhisperModel::Small),
        "medium" => Some(WhisperModel::Medium),
        "large-v3" => Some(WhisperModel::LargeV3),
        _ => None,
    }
}
