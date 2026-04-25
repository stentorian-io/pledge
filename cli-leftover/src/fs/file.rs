use std::path::Path;
use strum_macros::EnumIter;

#[derive(EnumIter)]
pub enum FileExt {
    Js,
    Jsx,
    Cjs,
    Mjs,
    Ts,
    Tsx,
    Cts,
    Mts,
    Other(String),
    None,
}

impl FileExt {
    pub fn from(path: &Path) -> Self {
        if let Some(extension) = path.extension() {
            let extension = match extension.to_str().unwrap() {
                "js" => Self::Js,
                "jsx" => Self::Jsx,
                "cjs" => Self::Cjs,
                "mjs" => Self::Mjs,
                "ts" => Self::Ts,
                "tsx" => Self::Tsx,
                "cts" => Self::Cts,
                "mts" => Self::Mts,
                other => Self::Other(other.into()),
            };

            return extension;
        }

        Self::None
    }

    pub fn is_ts(&self) -> bool {
        matches!(
            self,
            FileExt::Ts | FileExt::Tsx | FileExt::Cts | FileExt::Mts
        )
    }

    pub fn is_other(&self) -> bool {
        matches!(self, FileExt::Other(_))
    }

    pub fn to_value(&self) -> &str {
        match self {
            Self::Js => "js",
            Self::Jsx => "jsx",
            Self::Cjs => "cjs",
            Self::Mjs => "mjs",
            Self::Ts => "ts",
            Self::Tsx => "tsx",
            Self::Cts => "cts",
            Self::Mts => "mts",
            Self::Other(other) => other,
            Self::None => "",
        }
    }
}
