//! Byte offsets, as anyx spans count them, to and from the positions of the
//! Language Server Protocol, whose columns count UTF-16 code units.

use lsp_types::{Position, Range};

/// Where each line of a source starts.
pub struct LineIndex<'a> {
    source: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(source.match_indices('\n').map(|(index, _)| index + 1));
        LineIndex { source, starts }
    }

    pub fn position(&self, offset: usize) -> Position {
        let mut offset = offset.min(self.source.len());
        while !self.source.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.starts.partition_point(|start| *start <= offset) - 1;
        let column: usize = self.source[self.starts[line]..offset]
            .chars()
            .map(char::len_utf16)
            .sum();
        Position::new(line as u32, column as u32)
    }

    pub fn range(&self, start: usize, end: usize) -> Range {
        Range::new(self.position(start), self.position(end))
    }

    /// The byte offset of `position`; one past the end of a line is its end.
    pub fn offset(&self, position: Position) -> usize {
        let Some(&start) = self.starts.get(position.line as usize) else {
            return self.source.len();
        };
        let end = self
            .starts
            .get(position.line as usize + 1)
            .map_or(self.source.len(), |next| next - 1);
        let mut units = 0;
        for (index, character) in self.source[start..end].char_indices() {
            if units >= position.character as usize {
                return start + index;
            }
            units += character.len_utf16();
        }
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_count_utf16_units() {
        let source = "a\n彌勒 x\n𝄞y";
        let index = LineIndex::new(source);
        let x = source.find('x').unwrap();
        assert_eq!(index.position(x), Position::new(1, 3));
        assert_eq!(index.offset(Position::new(1, 3)), x);
        let y = source.find('y').unwrap();
        assert_eq!(index.position(y), Position::new(2, 2));
        assert_eq!(index.offset(Position::new(2, 2)), y);
        assert_eq!(index.offset(Position::new(9, 0)), source.len());
    }
}
