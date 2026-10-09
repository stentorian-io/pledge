//! Read TypeScript path aliases and resolve their configured source targets.

use std::path::{Path, PathBuf};

/// Parsed tsconfig.json path aliases.
#[derive(Debug, Clone, Default)]
pub struct TsconfigPaths {
    /// Base URL for non-relative module resolution.
    pub base_url: Option<String>,
    /// Path alias mappings: key is the alias pattern (e.g., "@/*"),
    /// value is the list of replacement paths (e.g., ["./src/*"]).
    pub paths: std::collections::HashMap<String, Vec<String>>,
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

    // Second pass: strip trailing commas before `}` or `]`.
    // Work on chars (not bytes) so multi-byte sequences remain intact.
    let mut result = String::with_capacity(out.len());
    let chars: Vec<char> = out.chars().collect();
    let mut j = 0;
    while j < chars.len() {
        let c = chars[j];
        if c == ',' {
            // Lookahead past whitespace for } or ]
            let mut k = j + 1;
            while k < chars.len() && chars[k].is_whitespace() {
                k += 1;
            }
            if k < chars.len() && (chars[k] == '}' || chars[k] == ']') {
                // Skip this trailing comma
                j += 1;
                continue;
            }
        }
        result.push(c);
        j += 1;
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

    let mut paths_map = std::collections::HashMap::new();
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
/// returns a list of candidate absolute paths to try.
///
/// Returns empty Vec if no alias matches.
pub fn resolve_tsconfig_alias(
    specifier: &str,
    tsconfig: &TsconfigPaths,
    pkg_root: &Path,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // Determine the base directory for resolution
    let base_dir = if let Some(ref base_url) = tsconfig.base_url {
        pkg_root.join(base_url)
    } else {
        pkg_root.to_path_buf()
    };

    for (pattern, replacements) in &tsconfig.paths {
        // Pattern can be exact match or wildcard match (e.g., "@/*")
        if pattern.contains('*') {
            // Wildcard pattern: "@/*" matches "@/utils", "@/components/Button", etc.
            let prefix = pattern.strip_suffix('*').unwrap_or(pattern);
            if let Some(remainder) = specifier.strip_prefix(prefix) {
                for replacement in replacements {
                    if replacement.contains('*') {
                        let replaced = replacement.replace('*', remainder);
                        let resolved = base_dir.join(replaced.trim_start_matches("./"));
                        // Add with standard extension probing
                        candidates.push(resolved.clone());
                        candidates.push(resolved.with_extension("ts"));
                        candidates.push(resolved.with_extension("tsx"));
                        candidates.push(resolved.with_extension("js"));
                        candidates.push(resolved.with_extension("jsx"));
                        // Directory index fallback
                        candidates.push(resolved.join("index.ts"));
                        candidates.push(resolved.join("index.tsx"));
                        candidates.push(resolved.join("index.js"));
                    } else {
                        let resolved = base_dir.join(replacement.trim_start_matches("./"));
                        candidates.push(resolved);
                    }
                }
            }
        } else {
            // Exact pattern match
            if specifier == pattern {
                for replacement in replacements {
                    let resolved = base_dir.join(replacement.trim_start_matches("./"));
                    candidates.push(resolved);
                }
            }
        }
    }

    candidates
}
