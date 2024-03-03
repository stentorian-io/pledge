use anyhow::{bail, Result};
use std::path::PathBuf;
use strum_macros::EnumIter;

#[derive(EnumIter)]
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

    pub fn is_other(&self) -> bool {
        match self {
            FileExt::OTHER(_) => true,
            _ => false,
        }
    }

    pub fn to_value(&self) -> &str {
        match self {
            Self::JS => "js",
            Self::JSX => "jsx",
            Self::CJS => "cjs",
            Self::MJS => "mjs",
            Self::TS => "ts",
            Self::TSX => "tsx",
            Self::CTS => "cts",
            Self::MTS => "mts",
            Self::OTHER(other) => other,
        }
    }
}
