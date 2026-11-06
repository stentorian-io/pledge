//! Read TypeScript path aliases and resolve their configured source targets.

use indexmap::IndexMap;
use std::path::{Path, PathBuf};

/// Parsed tsconfig.json path aliases.
#[derive(Debug, Clone, Default)]
pub struct TsconfigPaths {
    /// Base URL for non-relative module resolution.
    pub base_url: Option<String>,
    /// Path alias mappings: key is the alias pattern (e.g., "@/*"),
    /// value is the list of replacement paths (e.g., ["./src/*"]).
    pub paths: IndexMap<String, Vec<String>>,
}

/// ROB-04: Strip JSONC syntax (line comments, block comments, trailing commas) before
/// handing to serde_json. This covers the full JSONC feature set used by tsconfig.json.
///
/// The state machine correctly handles strings — comment markers inside string literals
/// are treated as regular characters. Unbalanced block-comment delimiters (e.g., `/*`
/// without a matching `*/`) are handled safely: the inner loop terminates at EOF.
///
/// Iterates over Unicode scalar values (chars) rather than raw bytes to preserve
/// multi-byte UTF-8 sequences — a tsconfig.json with non-ASCII identifiers, paths,
/// or comments (e.g., emoji, CJK) must survive stripping intact for serde_json.
pub(crate) fn strip_jsonc(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();
    let mut in_string = false;
    let mut string_char = '"';

    while let Some((_idx, c)) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                // Escape sequence — copy the next char verbatim (preserves e.g. \" inside strings).
                if let Some((_, next_c)) = chars.next() {
                    out.push(next_c);
                }
                continue;
            }
            if c == string_char {
                in_string = false;
            }
            continue;
        }

        // Start of a string
        if c == '"' || c == '\'' {
            in_string = true;
            string_char = c;
            out.push(c);
            continue;
        }

        // Line comment: //...
        if c == '/' && matches!(chars.peek(), Some((_, '/'))) {
            chars.next(); // consume second '/'
            while let Some(&(_, nc)) = chars.peek() {
                if nc == '\n' {
                    break;
                }
                chars.next();
            }
            continue;
        }

        // Block comment: /*... */
        if c == '/' && matches!(chars.peek(), Some((_, '*'))) {
            chars.next(); // consume '*'
            while let Some((_, nc)) = chars.next() {
                if nc == '*' && matches!(chars.peek(), Some((_, '/'))) {
                    chars.next(); // consume '/'
                    break;
                }
            }
            continue;
        }

        out.push(c);
    }

    // Trailing commas are syntax only outside strings, including escaped quotes.
    let mut result = String::with_capacity(out.len());
    let mut chars = out.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            result.push(c);
            if c == '\\' {
                if let Some(escaped) = chars.next() {
                    result.push(escaped);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
        }
        if c == ',' {
            let next_token = chars.clone().find(|next| !next.is_whitespace());
            if matches!(next_token, Some('}' | ']')) {
                continue;
            }
        }
        result.push(c);
    }

    result
}

/// Parse `tsconfig.json` from `pkg_root` and extract `compilerOptions.paths` and `compilerOptions.baseUrl`.
///
/// Returns `None` if tsconfig.json doesn't exist or is unparseable.
/// This is best-effort — missing fields are silently ignored.
pub fn parse_tsconfig_paths(pkg_root: &Path) -> Option<TsconfigPaths> {
    let tsconfig_path = pkg_root.join("tsconfig.json");
    let content = std::fs::read_to_string(&tsconfig_path).ok()?;
    // ROB-04: tsconfig.json is JSONC — strip line comments, block comments, and
    // trailing commas before handing to serde_json.
    let stripped = strip_jsonc(&content);
    let json: serde_json::Value = serde_json::from_str(&stripped).ok()?;

    let compiler_opts = json.get("compilerOptions")?;

    let base_url = compiler_opts
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut paths_map = IndexMap::new();
    if let Some(paths_obj) = compiler_opts.get("paths").and_then(|v| v.as_object()) {
        for (key, value) in paths_obj {
            if let Some(arr) = value.as_array() {
                let replacements: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if !replacements.is_empty() {
                    paths_map.insert(key.clone(), replacements);
                }
            }
        }
    }

    Some(TsconfigPaths {
        base_url,
        paths: paths_map,
    })
}

/// Attempt to resolve a module specifier using tsconfig path aliases.
///
/// Given a specifier like `@/utils` and paths like `{"@/*": ["./src/*"]}`,
/// returns the selected replacement paths in their configured fallback order.
/// The ecosystem resolver applies extension and directory probing. Exact aliases
/// win, then the longest matching wildcard prefix; ties retain declaration order.
///
/// Returns empty Vec if no alias matches.
pub fn resolve_tsconfig_alias(
    specifier: &str,
    tsconfig: &TsconfigPaths,
    pkg_root: &Path,
) -> Vec<PathBuf> {
    let base_dir = tsconfig
        .base_url
        .as_ref()
        .map_or_else(|| pkg_root.to_path_buf(), |base| pkg_root.join(base));

    let selection = if let Some(replacements) = tsconfig.paths.get(specifier) {
        Some((replacements, None))
    } else {
        let mut selection = None;
        let mut prefix_length = None;
        for (pattern, replacements) in &tsconfig.paths {
            let Some((prefix, suffix)) = pattern.split_once('*') else {
                continue;
            };
            if suffix.contains('*') || prefix_length.is_some_and(|length| length >= prefix.len()) {
                continue;
            }
            let Some(matched) = specifier
                .strip_prefix(prefix)
                .and_then(|remainder| remainder.strip_suffix(suffix))
            else {
                continue;
            };
            prefix_length = Some(prefix.len());
            selection = Some((replacements, Some(matched)));
        }
        selection
    };

    let Some((replacements, matched)) = selection else {
        return Vec::new();
    };
    replacements
        .iter()
        .map(|replacement| {
            let replacement = matched.map_or_else(
                || replacement.clone(),
                |matched| replacement.replacen('*', matched, 1),
            );
            base_dir.join(replacement)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::strip_jsonc;

    #[test]
    fn comments_and_trailing_commas_do_not_change_string_contents() {
        let source = r#"{
            // Comments and actual trailing commas are removed.
            "paths": ["leaf,].ts", "a,}b", "quote\",]", "雪,].ts",],
            "url": "https://example.invalid/a,]", /* block */
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(&strip_jsonc(source)).unwrap();

        assert_eq!(
            parsed["paths"],
            serde_json::json!(["leaf,].ts", "a,}b", "quote\",]", "雪,].ts"])
        );
        assert_eq!(parsed["url"], "https://example.invalid/a,]");
    }
}
