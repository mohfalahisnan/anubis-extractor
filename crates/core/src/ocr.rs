//! OCR backend on top of the `ocrs` crate (rten ONNX runtime).
//!
//! Two model files (detection + recognition) are downloaded on first use and
//! cached in `<cache_dir>/ocr/`. Once an `OcrBackend` is constructed the
//! `ocrs::OcrEngine` is held for its lifetime.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ocrs::{ImageSource, OcrEngine, OcrEngineParams};

use crate::download::{ensure_file, EventSink};
use crate::types::OcrLine;
use crate::ExtractorError;

const DETECTION_MODEL_URL: &str =
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten";
const RECOGNITION_MODEL_URL: &str =
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten";
const DETECTION_MODEL_FILE: &str = "text-detection.rten";
const RECOGNITION_MODEL_FILE: &str = "text-recognition.rten";

#[derive(Clone)]
pub(crate) struct OcrBackend {
    inner: Arc<OcrInner>,
}

struct OcrInner {
    cache_dir: PathBuf,
    engine: Mutex<Option<OcrEngine>>,
}

impl OcrBackend {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(OcrInner {
                cache_dir,
                engine: Mutex::new(None),
            }),
        }
    }

    /// Used as a backend version tag in the sidecar cache key.
    /// Wired in for v0.2 (content-hashed cache keys); v0.1 uses mtime-based freshness.
    #[allow(dead_code)]
    pub fn version_tag(&self) -> &'static str {
        "ocrs-0.12"
    }

    pub fn run(
        &self,
        image_bytes: &[u8],
        sink: Option<&EventSink<'_>>,
    ) -> Result<crate::types::OcrResult, ExtractorError> {
        if image_bytes.is_empty() {
            return Ok(crate::types::OcrResult {
                text: String::new(),
                lines: vec![],
                sidecar_path: None,
                cache_hit: false,
            });
        }

        self.ensure_engine(sink)?;
        let guard = self.inner.engine.lock().expect("engine mutex poisoned");
        let engine = guard.as_ref().expect("engine initialized");

        let decoded = image::load_from_memory(image_bytes)
            .map_err(|e| ExtractorError::Ocr(format!("image decode failed: {e}")))?;
        let rgb = decoded.to_rgb8();
        let (width, height) = rgb.dimensions();
        let raw = rgb.into_raw();

        let source = ImageSource::from_bytes(&raw, (width, height))
            .map_err(|e| ExtractorError::Ocr(format!("ocr image source: {e:?}")))?;
        let input = engine
            .prepare_input(source)
            .map_err(|e| ExtractorError::Ocr(format!("ocr prepare_input: {e}")))?;

        let text = engine
            .get_text(&input)
            .map_err(|e| ExtractorError::Ocr(format!("ocr get_text: {e}")))?;

        let lines = if text.trim().is_empty() {
            vec![]
        } else {
            vec![OcrLine { bbox: [0, 0, width, height], text: text.clone() }]
        };

        Ok(crate::types::OcrResult {
            text,
            lines,
            sidecar_path: None,
            cache_hit: false,
        })
    }

    fn ensure_engine(&self, sink: Option<&EventSink<'_>>) -> Result<(), ExtractorError> {
        let mut guard = self.inner.engine.lock().expect("engine mutex poisoned");
        if guard.is_some() {
            return Ok(());
        }
        let dir = self.inner.cache_dir.join("ocr");
        std::fs::create_dir_all(&dir)?;
        let det = dir.join(DETECTION_MODEL_FILE);
        let rec = dir.join(RECOGNITION_MODEL_FILE);
        ensure_file(&det, DETECTION_MODEL_URL, "ocr-detection", "OCR text detection model", sink)
            .map_err(ExtractorError::Download)?;
        ensure_file(
            &rec,
            RECOGNITION_MODEL_URL,
            "ocr-recognition",
            "OCR text recognition model",
            sink,
        )
        .map_err(ExtractorError::Download)?;
        let det_model = rten::Model::load_file(&det)
            .map_err(|e| ExtractorError::Ocr(format!("load detection model: {e}")))?;
        let rec_model = rten::Model::load_file(&rec)
            .map_err(|e| ExtractorError::Ocr(format!("load recognition model: {e}")))?;
        let engine = OcrEngine::new(OcrEngineParams {
            detection_model: Some(det_model),
            recognition_model: Some(rec_model),
            ..Default::default()
        })
        .map_err(|e| ExtractorError::Ocr(format!("ocr engine init: {e}")))?;
        *guard = Some(engine);
        Ok(())
    }
}
