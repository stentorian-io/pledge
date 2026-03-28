pub mod shape;

use std::collections::HashMap;
use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Major,
    Minor,
    Patch,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub rule: &'static str,
    pub file: PathBuf,
    pub line: u32,
    pub col: u32,
    pub message: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportKind {
    Named,
    Default,
    Namespace,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ExportSymbol {
    pub name: String,
    pub kind: ExportKind,
    pub file: PathBuf,
    pub line: u32,
    pub col: u32,
    pub type_signature: Option<String>,
    pub type_shape: Option<shape::TypeShape>,
}

#[allow(dead_code)]
pub type ExportMap = HashMap<String, ExportSymbol>;
