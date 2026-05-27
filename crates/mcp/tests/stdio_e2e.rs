//! Spawn the stdio server in-process and round-trip an initialize + tools/list call.

use std::path::PathBuf;

use anubis_extractor::{Config, Extractor, WhisperModel};
use anubis_extractor_mcp::server::ExtractorServer;
use rmcp::{model::ClientInfo, ClientHandler, ServiceExt};

/// Minimal no-op client handler required by rmcp to drive the handshake.
#[derive(Debug, Clone, Default)]
struct DummyClient;

impl ClientHandler for DummyClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::default()
    }
}

// Note: this test does NOT exercise OCR or transcription — it just confirms
// the server starts and tools/list returns the three expected tool names.
// No model downloads needed, so this is NOT #[ignore].
#[tokio::test]
async fn tools_list_returns_three_tools() {
    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/.cache");
    let extractor = Extractor::new(Config {
        cache_dir: cache,
        whisper_model: WhisperModel::Tiny,
    })
    .unwrap();
    let server = ExtractorServer::new(extractor);

    let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);

    // Spawn the server — it will serve the client_transport side and wait until closed.
    let server_task = tokio::spawn(async move {
        server
            .serve(server_transport)
            .await
            .expect("server serve failed")
            .waiting()
            .await
            .expect("server waiting failed");
    });

    // Run the handshake from the client side; this is the proven rmcp test pattern.
    let client = DummyClient
        .serve(client_transport)
        .await
        .expect("client handshake failed");

    // tools/list round-trip
    let tools = client
        .list_tools(Default::default())
        .await
        .expect("tools/list failed");

    let names: Vec<&str> = tools.tools.iter().map(|t| t.name.as_ref()).collect();

    assert!(
        names.contains(&"extractor_transcribe"),
        "missing extractor_transcribe; got: {names:?}"
    );
    assert!(
        names.contains(&"extractor_ocr"),
        "missing extractor_ocr; got: {names:?}"
    );
    assert!(
        names.contains(&"extractor_extract_text"),
        "missing extractor_extract_text; got: {names:?}"
    );

    // Clean shutdown: cancel the client so the server's read end sees EOF.
    client.cancel().await.expect("cancel failed");
    server_task.await.expect("server task panicked");
}
