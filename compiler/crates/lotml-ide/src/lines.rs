//! Byte offsets as an editor counts positions: lines from 0, and columns in UTF-16 code units —
//! the Language Server Protocol's default — or in bytes, when the client accepts UTF-8.

/// What a column counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16,
}

/// Where each line of a text starts.
pub struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    pub fn new(text: &'a str) -> Lines<'a> {
        let starts = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
        Lines { text, starts }
    }

    /// How many lines the text has; a final line break starts an empty last line.
    pub fn count(&self) -> usize {
        self.starts.len()
    }

    /// A line's text, without its line break; empty past the last line.
    pub fn line(&self, line: usize) -> &'a str {
        let Some(&start) = self.starts.get(line) else { return "" };
        let end = self.starts.get(line + 1).map_or(self.text.len(), |next| next - 1);
        self.text[start..end].strip_suffix('\r').unwrap_or(&self.text[start..end])
    }

    /// The line (from 0) and column of a byte offset, clamped to the text.
    pub fn position(&self, offset: u32, encoding: Encoding) -> (u32, u32) {
        let mut offset = (offset as usize).min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.starts.partition_point(|&start| start <= offset) - 1;
        let before = &self.text[self.starts[line]..offset];
        let column = match encoding {
            Encoding::Utf8 => before.len(),
            Encoding::Utf16 => before.encode_utf16().count(),
        };
        (narrow(line), narrow(column))
    }

    /// The byte offset of a line and column. A column past the end of its line is the line's
    /// end, and a line past the last is the end of the text, as the protocol asks.
    pub fn offset(&self, line: u32, column: u32, encoding: Encoding) -> u32 {
        let Some(&start) = self.starts.get(line as usize) else { return narrow(self.text.len()) };
        let mut counted = 0;
        for (i, c) in self.line(line as usize).char_indices() {
            if counted >= column as usize {
                return narrow(start + i);
            }
            counted += match encoding {
                Encoding::Utf8 => c.len_utf8(),
                Encoding::Utf16 => c.len_utf16(),
            };
        }
        narrow(start + self.line(line as usize).len())
    }
}

fn narrow(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_utf16_units_or_bytes() {
        // `é` is two bytes and one UTF-16 unit; `𝄞` is four bytes and two units.
        let text = "a = 1\ns = \"é𝄞x\"\n";
        let lines = Lines::new(text);
        let x = u32::try_from(text.find('x').unwrap()).unwrap();
        assert_eq!(lines.position(x, Encoding::Utf16), (1, 8));
        assert_eq!(lines.position(x, Encoding::Utf8), (1, 11));
        assert_eq!(lines.offset(1, 8, Encoding::Utf16), x);
        assert_eq!(lines.offset(1, 11, Encoding::Utf8), x);
    }

    #[test]
    fn positions_past_the_end_are_clamped() {
        let lines = Lines::new("ab\ncd");
        assert_eq!(lines.offset(0, 99, Encoding::Utf16), 2, "the end of the first line");
        assert_eq!(lines.offset(9, 0, Encoding::Utf16), 5, "the end of the text");
        assert_eq!(lines.position(99, Encoding::Utf16), (1, 2));
    }

    #[test]
    fn lines_drop_their_line_breaks() {
        let lines = Lines::new("one\r\ntwo\n");
        assert_eq!(lines.count(), 3);
        assert_eq!(lines.line(0), "one");
        assert_eq!(lines.line(1), "two");
        assert_eq!(lines.line(2), "");
        assert_eq!(lines.line(7), "");
    }

    #[test]
    fn an_offset_inside_a_character_is_its_start() {
        let lines = Lines::new("é");
        assert_eq!(lines.position(1, Encoding::Utf8), (0, 0));
    }
}
