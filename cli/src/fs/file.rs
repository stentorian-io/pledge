use anyhow::{bail, Result};
use std::path::PathBuf;

pub enum FileExt {
    JS,
    JSX,
    CJS,
    MJS,
    TS,
    TSX,
    CTS,
    MTS,
    OTHER(String),
}

impl FileExt {
    pub fn from(path: &PathBuf) -> Result<Self> {
        if let Some(extension) = path.extension() {
            let extension = match extension.to_str().unwrap() {
                "js" => Self::JS,
                "jsx" => Self::JSX,
                "cjs" => Self::CJS,
                "mjs" => Self::MJS,
                "ts" => Self::TS,
                "tsx" => Self::TSX,
                "cts" => Self::CTS,
                "mts" => Self::MTS,
                other => Self::OTHER(other.into()),
            };

            return Ok(extension);
        }

        bail!("Cannot get file extension from '{}'.", path.display());
    }

    pub fn is_ts(&self) -> bool {
        match self {
            FileExt::TS | FileExt::TSX | FileExt::CTS | FileExt::MTS => true,
            _ => false,
        }
    }
}
