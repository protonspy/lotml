//! Positions in source text, as UTF-8 byte offsets.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        let narrow = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Span { start: narrow(start), end: narrow(end) }
    }

    /// The smallest span covering both.
    pub fn to(self, other: Span) -> Span {
        Span { start: self.start.min(other.start), end: self.end.max(other.end) }
    }

    pub fn range(self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// Line and column (both from 1, the column in characters) of a byte offset.
pub fn line_column(text: &str, offset: u32) -> (usize, usize) {
    let offset = (offset as usize).min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[start..].chars().count() + 1)
}
