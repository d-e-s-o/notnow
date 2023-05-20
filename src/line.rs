// Copyright (C) 2023 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::min;
use std::ops::RangeFrom;

use unicode_segmentation::UnicodeSegmentation as _;


/// Find the byte index that maps to the given character position.
fn byte_index(string: &str, position: usize) -> usize {
  let extended = true;
  string
    .grapheme_indices(extended)
    .map(|(byte_idx, _grapheme)| byte_idx)
    .nth(position)
    .unwrap_or(string.len())
}

/// Find the character index that maps to the given byte position.
#[cfg(any(test, feature = "readline"))]
fn char_index(string: &str, byte_position: usize) -> usize {
  let extended = true;
  string
    .grapheme_indices(extended)
    .take_while(|(idx, grapheme)| byte_position >= idx + grapheme.len())
    .count()
}


/// A line of text with an associate selection.
///
/// We use the word character ("char") loosely in this module, referring
/// to what a user would intuitively describe as a character. Really
/// it's a grapheme cluster in Unicode speak. All indexes, unless
/// explicitly denoted otherwise, are relative to these characters and
/// not to bytes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Line {
  /// The string representing the line.
  line: String,
  /// The "character" index of the selection.
  selection: usize,
}

#[allow(unused)]
impl Line {
  /// Create a `Line` from the given string, selecting the very first
  /// character.
  pub fn from_string<S>(line: S) -> Self
  where
    S: Into<String>,
  {
    Self {
      line: line.into(),
      // An index of zero is always valid, even if `line` was empty.
      selection: 0,
    }
  }

  /// Select the first character.
  pub fn select_start(self) -> Self {
    let mut slf = self;
    slf.selection = 0;
    slf
  }

  /// Move the selection towards the end of the line, i.e., past the
  /// last character.
  pub fn select_end(self) -> Self {
    let mut slf = self;
    slf.selection = slf.len();
    slf
  }

  /// Select the next character, if any.
  pub fn select_next(self) -> Self {
    let mut slf = self;
    slf.selection = min(slf.selection + 1, slf.len());
    slf
  }

  /// Select the previous character, if any.
  pub fn select_prev(self) -> Self {
    let mut slf = self;
    slf.selection = min(slf.selection.saturating_sub(1), slf.len());
    slf
  }

  /// Select a character based on its byte index.
  #[cfg(feature = "readline")]
  pub fn select_byte_index(self, byte_index: usize) -> Self {
    let mut slf = self;
    slf.selection = char_index(&slf.line, byte_index);
    slf
  }

  /// Insert a character into the line at the current selection.
  pub fn insert_char(&mut self, c: char) {
    let byte_index = byte_index(&self.line, self.selection);
    let () = self.line.insert(byte_index, c);
    self.selection = min(self.selection + 1, self.len());
  }

  /// Remove the currently selected character from the line.
  pub fn remove_char(&mut self) {
    if self.selection >= self.len() {
      return
    }

    let byte_index = byte_index(&self.line, self.selection);
    let _removed = self.line.remove(byte_index);
    self.selection = min(self.selection, self.len());
  }

  /// Retrieve a sub-string of the line.
  pub fn substr(&self, range: RangeFrom<usize>) -> &str {
    let range = RangeFrom {
      start: byte_index(&self.line, range.start),
    };
    self.line.get(range).unwrap_or("")
  }

  /// Retrieve the number of characters in the line.
  #[inline]
  pub fn len(&self) -> usize {
    let extended = true;
    self.line.graphemes(extended).count()
  }

  /// Retrieve the line's underlying `str`.
  #[inline]
  pub fn as_str(&self) -> &str {
    &self.line
  }

  /// Retrieve the current selection index.
  #[inline]
  pub fn selection(&self) -> usize {
    self.selection
  }

  /// Retrieve the current selection expressed as a byte index.
  #[inline]
  #[cfg(feature = "readline")]
  pub fn selection_byte_index(&self) -> usize {
    byte_index(&self.line, self.selection)
  }

  /// Convert the object into a `String`, discarding selection
  /// information.
  pub fn into_string(self) -> String {
    self.line
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  /// Check that our "character" indexing works as it should.
  #[test]
  fn byte_indexing() {
    let s = "";
    assert_eq!(byte_index(s, 0), 0);
    assert_eq!(byte_index(s, 1), 0);

    let s = "s";
    assert_eq!(byte_index(s, 0), 0);
    assert_eq!(byte_index(s, 1), 1);
    assert_eq!(byte_index(s, 2), 1);

    let s = "foobar";
    assert_eq!(byte_index(s, 0), 0);
    assert_eq!(byte_index(s, 1), 1);
    assert_eq!(byte_index(s, 5), 5);
    assert_eq!(byte_index(s, 6), 6);
    assert_eq!(byte_index(s, 7), 6);

    let s = "⚠️attn⚠️";
    assert_eq!(byte_index(s, 0), 0);
    assert_eq!(byte_index(s, 1), 6);
    assert_eq!(byte_index(s, 2), 7);
    assert_eq!(byte_index(s, 3), 8);
    assert_eq!(byte_index(s, 5), 10);
    assert_eq!(byte_index(s, 6), 16);
    assert_eq!(byte_index(s, 7), 16);
  }

  /// Check that our "character" indexing works as it should.
  #[test]
  fn char_indexing() {
    let s = "";
    assert_eq!(char_index(s, 0), 0);
    assert_eq!(char_index(s, 1), 0);

    let s = "s";
    assert_eq!(char_index(s, 0), 0);
    assert_eq!(char_index(s, 1), 1);
    assert_eq!(char_index(s, 2), 1);

    let s = "foobar";
    assert_eq!(char_index(s, 0), 0);
    assert_eq!(char_index(s, 1), 1);
    assert_eq!(char_index(s, 5), 5);
    assert_eq!(char_index(s, 6), 6);
    assert_eq!(char_index(s, 7), 6);

    let s = "⚠️attn⚠️";
    assert_eq!(char_index(s, 0), 0);
    assert_eq!(char_index(s, 1), 0);
    assert_eq!(char_index(s, 6), 1);
    assert_eq!(char_index(s, 7), 2);
  }

  /// Check that `Line::substr` behaves as it should.
  #[test]
  fn line_substr() {
    let line = Line::default();
    assert_eq!(line.substr(0..), "");
    assert_eq!(line.substr(1..), "");
    assert_eq!(line.substr(2..), "");

    let line = Line::from_string("s");
    assert_eq!(line.substr(0..), "s");
    assert_eq!(line.substr(1..), "");
    assert_eq!(line.substr(2..), "");

    let line = Line::from_string("string");
    assert_eq!(line.substr(0..), "string");
    assert_eq!(line.substr(1..), "tring");
    assert_eq!(line.substr(2..), "ring");
    assert_eq!(line.substr(5..), "g");
    assert_eq!(line.substr(6..), "");
  }

  /// Check that `Line::len` works as expected.
  #[test]
  fn line_length() {
    let line = Line::default();
    assert_eq!(line.len(), 0);

    let line = Line::from_string("⚠️attn⚠️");
    assert_eq!(line.len(), 6);
  }
}
