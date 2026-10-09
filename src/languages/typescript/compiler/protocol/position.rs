//! Translate tree-sitter's byte columns to LSP's UTF-16 code units.

use crate::languages::typescript::compiler::LspError;
use async_lsp::lsp_types::{Position, Url};

pub(super) async fn source_position(uri: &Url, line: u32, col: u32) -> Result<Position, LspError> {
    let file = uri
        .to_file_path()
        .map_err(|_| LspError::RequestFailed(format!("invalid source URI: {uri}")))?;
    let source = tokio::fs::read_to_string(&file).await.map_err(|error| {
        LspError::RequestFailed(format!("cannot read {}: {error}", file.display()))
    })?;
    position_in_source(&source, line, col)
}

fn position_in_source(source: &str, line: u32, col: u32) -> Result<Position, LspError> {
    let row = line
        .checked_sub(1)
        .ok_or_else(|| LspError::RequestFailed("source line must be one-based".into()))?;
    let byte_column = col
        .checked_sub(1)
        .ok_or_else(|| LspError::RequestFailed("source column must be one-based".into()))?
        as usize;
    let text = source
        .split('\n')
        .nth(row as usize)
        .ok_or_else(|| LspError::RequestFailed("source line is outside the document".into()))?;
    let prefix = text.get(..byte_column).ok_or_else(|| {
        LspError::RequestFailed("source column is not a valid UTF-8 position".into())
    })?;
    Ok(Position::new(row, prefix.encode_utf16().count() as u32))
}

#[cfg(test)]
mod tests {
    use crate::languages::typescript::compiler::protocol::position::*;

    #[test]
    fn columns_preserve_ascii_bmp_astral_and_crlf_positions() {
        for prefix in ["", "ascii ", "café ", "🧭 ", "e\u{301} ", "🧭 café "] {
            let source = format!("first\r\n{prefix}target");
            assert_eq!(
                position_in_source(&source, 2, prefix.len() as u32 + 1).unwrap(),
                Position::new(1, prefix.encode_utf16().count() as u32)
            );
        }
    }

    #[test]
    fn malformed_source_positions_fail_explicitly() {
        for (line, col) in [(0, 1), (1, 0), (2, 1), (1, 2), (1, 10)] {
            assert!(position_in_source("🧭", line, col).is_err(), "{line}:{col}");
        }
    }
}
