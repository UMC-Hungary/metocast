//! Hungarian speech → English text for Metocast.
//!
//! The pieces are deliberately independent so the core server can reuse them for live
//! transcription and the CLI can run them against recorded files:
//! decoding ([`decode`]), resampling ([`resample`]), speech-onset detection for latency
//! measurement ([`onset`]), the OpenAI clients ([`realtime`], [`whisper`]), turning a stream of
//! text deltas into committed segments ([`segmenter`]) and caption rendering ([`captions`]).

pub mod captions;
pub mod decode;
pub mod framer;
pub mod onset;
pub mod realtime;
pub mod resample;
pub mod retime;
pub mod segmenter;
pub mod whisper;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("decode: {0}")]
    Decode(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("websocket: {0}")]
    WebSocket(String),
    #[error("http: {0}")]
    Http(String),
    #[error("api: {0}")]
    Api(String),
}

pub type Result<T> = std::result::Result<T, Error>;
