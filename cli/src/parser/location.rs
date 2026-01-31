//! Source location tracking types.

use std::fmt;

/// A position in source code (line and column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceLocation {
    /// Line number (1-indexed).
    pub line: usize,
    /// Column number (1-indexed).
    pub column: usize,
    /// Byte offset from the start of the source.
    pub offset: usize,
}

impl SourceLocation {
    /// Creates a new source location.
    pub fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }

    /// Creates a source location from a tree-sitter Point.
    pub fn from_point(point: tree_sitter::Point, offset: usize) -> Self {
        Self {
            // tree-sitter uses 0-indexed lines and columns, we use 1-indexed
            line: point.row + 1,
            column: point.column + 1,
            offset,
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// A range in source code (start and end positions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceRange {
    /// Start position of the range.
    pub start: SourceLocation,
    /// End position of the range.
    pub end: SourceLocation,
}

impl SourceRange {
    /// Creates a new source range.
    pub fn new(start: SourceLocation, end: SourceLocation) -> Self {
        Self { start, end }
    }

    /// Creates a source range from tree-sitter node positions.
    pub fn from_node(node: &tree_sitter::Node) -> Self {
        Self {
            start: SourceLocation::from_point(node.start_position(), node.start_byte()),
            end: SourceLocation::from_point(node.end_position(), node.end_byte()),
        }
    }

    /// Returns the byte length of this range.
    pub fn byte_len(&self) -> usize {
        self.end.offset.saturating_sub(self.start.offset)
    }

    /// Checks if this range contains a given location.
    pub fn contains(&self, loc: &SourceLocation) -> bool {
        loc.offset >= self.start.offset && loc.offset < self.end.offset
    }

    /// Checks if this range overlaps with another range.
    pub fn overlaps(&self, other: &SourceRange) -> bool {
        self.start.offset < other.end.offset && other.start.offset < self.end.offset
    }
}

impl fmt::Display for SourceRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_location_display() {
        let loc = SourceLocation::new(10, 5, 100);
        assert_eq!(format!("{}", loc), "10:5");
    }

    #[test]
    fn test_source_range_byte_len() {
        let range = SourceRange::new(
            SourceLocation::new(1, 1, 0),
            SourceLocation::new(1, 10, 9),
        );
        assert_eq!(range.byte_len(), 9);
    }

    #[test]
    fn test_source_range_contains() {
        let range = SourceRange::new(
            SourceLocation::new(1, 1, 0),
            SourceLocation::new(1, 10, 10),
        );
        assert!(range.contains(&SourceLocation::new(1, 5, 5)));
        assert!(!range.contains(&SourceLocation::new(1, 15, 15)));
    }

    #[test]
    fn test_source_range_overlaps() {
        let range1 = SourceRange::new(
            SourceLocation::new(1, 1, 0),
            SourceLocation::new(1, 10, 10),
        );
        let range2 = SourceRange::new(
            SourceLocation::new(1, 5, 5),
            SourceLocation::new(1, 15, 15),
        );
        let range3 = SourceRange::new(
            SourceLocation::new(2, 1, 20),
            SourceLocation::new(2, 10, 30),
        );

        assert!(range1.overlaps(&range2));
        assert!(range2.overlaps(&range1));
        assert!(!range1.overlaps(&range3));
    }
}
