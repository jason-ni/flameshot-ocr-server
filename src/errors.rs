use thiserror::Error;
use tesseract::TesseractError;

#[derive(Error, Debug)]
pub enum OcrError {
    #[error("tesseract error")]
    TesseractError(#[from] TesseractError),
    #[error("io error")]
    IoError(#[from] std::io::Error),
    #[error("llama error")]
    LlamaError(#[from] Box<dyn std::error::Error>),
}
