// Copyright (C) 2023-2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::min;
use std::ops::ControlFlow;
use std::ops::RangeFrom;

use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;


/// Find the byte index that maps to the given character position.
fn byte_index(string: &str, position: usize) -> usize {
  let extended = true;
  let result =
    string
      .grapheme_indices(extended)
      .try_fold(0usize, |total_width, (byte_idx, grapheme)| {
        if total_width >= position {
          ControlFlow::Break(byte_idx)
        } else {
          ControlFlow::Continue(total_width + grapheme.width())
        }
      });

  match result {
    ControlFlow::Break(byte_idx) => byte_idx,
    ControlFlow::Continue(_) => string.len(),
  }
}

/// Find the character index that maps to the given byte position.
fn char_index(string: &str, byte_position: usize) -> usize {
  let extended = true;
  string
    .grapheme_indices(extended)
    .map_while(|(idx, grapheme)| {
      if byte_position >= idx + grapheme.len() {
        Some(grapheme.width())
      } else {
        None
      }
    })
    .sum()
}


/// A text with an associated selection.
///
/// We use the word character ("char") loosely in this module, referring
/// to what a user would intuitively describe as a character. Really
/// it's a grapheme cluster in Unicode speak. All indexes, unless
/// explicitly denoted otherwise, are relative to these characters and
/// not to bytes.
///
/// Please note that at the moment, selections take into account
/// character width. That is arguably more of a property pertaining the
/// specific output in use, and so we are effectively specific to
/// terminal based use cases at the moment.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EditableText {
  /// The string representing the text.
  text: String,
  /// The "character" index of the selection.
  selection: usize,
}

#[allow(unused)]
impl EditableText {
  /// Create a `EditableText` from the given string, selecting the very first
  /// character.
  pub fn from_string<S>(text: S) -> Self
  where
    S: Into<String>,
  {
    Self {
      text: text.into(),
      // An index of zero is always valid, even if `text` was empty.
      selection: 0,
    }
  }

  /// Select the first character.
  pub fn select_start(self) -> Self {
    let mut slf = self;
    slf.selection = 0;
    slf
  }

  /// Move the selection towards the end of the text, i.e., past the
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
    slf.selection = char_index(&slf.text, byte_index);
    slf
  }

  /// Insert a character into the text at the current selection.
  pub fn insert_char(&mut self, c: char) {
    let byte_index = byte_index(&self.text, self.selection);
    let () = self.text.insert(byte_index, c);
    self.selection = min(self.selection + 1, self.len());
  }

  /// Remove the currently selected character from the text.
  pub fn remove_char(&mut self) {
    if self.selection >= self.len() {
      return
    }

    let byte_index = byte_index(&self.text, self.selection);
    let _removed = self.text.remove(byte_index);
    self.selection = min(self.selection, self.len());
  }

  /// Retrieve a sub-string of the text.
  pub fn substr(&self, range: RangeFrom<usize>) -> &str {
    let range = RangeFrom {
      start: byte_index(&self.text, range.start),
    };
    self.text.get(range).unwrap_or("")
  }

  /// Retrieve the number of characters in the text.
  #[inline]
  pub fn len(&self) -> usize {
    char_index(&self.text, self.text.len())
  }

  /// Retrieve the text's underlying `str`.
  #[inline]
  pub fn as_str(&self) -> &str {
    &self.text
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
    byte_index(&self.text, self.selection)
  }

  /// Convert the object into a `String`, discarding selection
  /// information.
  pub fn into_string(self) -> String {
    self.text
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

    let s = "a｜b";
    assert_eq!(byte_index(s, 0), 0);
    assert_eq!(byte_index(s, 1), 1);
    assert_eq!(byte_index(s, 2), 4);
    assert_eq!(byte_index(s, 3), 4);
    assert_eq!(byte_index(s, 4), 5);
    assert_eq!(byte_index(s, 5), 5);
    assert_eq!(byte_index(s, 6), 5);
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

    let s = "a｜b";
    assert_eq!(char_index(s, 0), 0);
    assert_eq!(char_index(s, 1), 1);
    assert_eq!(char_index(s, 2), 1);
    assert_eq!(char_index(s, 3), 1);
    assert_eq!(char_index(s, 4), 3);
    assert_eq!(char_index(s, 5), 4);
    assert_eq!(char_index(s, 6), 4);
  }

  /// Check that `EditableText::substr` behaves as it should.
  #[test]
  fn text_substr() {
    let text = EditableText::default();
    assert_eq!(text.substr(0..), "");
    assert_eq!(text.substr(1..), "");
    assert_eq!(text.substr(2..), "");

    let text = EditableText::from_string("s");
    assert_eq!(text.substr(0..), "s");
    assert_eq!(text.substr(1..), "");
    assert_eq!(text.substr(2..), "");

    let text = EditableText::from_string("string");
    assert_eq!(text.substr(0..), "string");
    assert_eq!(text.substr(1..), "tring");
    assert_eq!(text.substr(2..), "ring");
    assert_eq!(text.substr(5..), "g");
    assert_eq!(text.substr(6..), "");
  }

  /// Check that `EditableText::len` works as expected.
  #[test]
  fn text_length() {
    let text = EditableText::default();
    assert_eq!(text.len(), 0);

    let text = EditableText::from_string("⚠️attn⚠️");
    assert_eq!(text.len(), 6);
  }
}
