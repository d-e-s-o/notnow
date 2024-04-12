// Copyright (C) 2023-2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! Please note that we use the word character ("char") loosely in this
//! module, referring to what a user would intuitively describe as a
//! character. Really it's a grapheme cluster in Unicode speak. All
//! indexes, unless explicitly denoted otherwise, are relative to these
//! characters and not to bytes.

use std::cmp::min;
use std::ops::Bound::Excluded;
use std::ops::Bound::Included;
use std::ops::Bound::Unbounded;
use std::ops::ControlFlow;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ops::RangeBounds;
use std::slice::SliceIndex;

use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;


/// Find the byte index that maps to the given character position.
fn byte_index(string: &str, position: usize) -> usize {
  let extended = true;
  let result = string.grapheme_indices(extended).enumerate().try_for_each(
    |(char_pos, (byte_idx, _grapheme))| {
      if char_pos >= position {
        ControlFlow::Break(byte_idx)
      } else {
        ControlFlow::Continue(())
      }
    },
  );

  match result {
    ControlFlow::Break(byte_idx) => byte_idx,
    ControlFlow::Continue(()) => string.len(),
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


/// Some Unicode aware text.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Text {
  text: String,
}

impl Text {
  /// Create a `Text` from the given string.
  pub fn from_string<S>(text: S) -> Self
  where
    S: Into<String>,
  {
    Self { text: text.into() }
  }

  /// Retrieve a sub-string of the text.
  pub fn substr<R>(&self, range: R) -> &str
  where
    R: RangeBounds<usize>,
  {
    fn get(text: &str, range: impl SliceIndex<str, Output = str>) -> &str {
      text.get(range).unwrap_or("")
    }

    let start = range.start_bound();
    let end = range.end_bound();

    match (start, end) {
      (Included(start), Unbounded) => {
        let range = byte_index(&self.text, *start)..;
        get(&self.text, range)
      },
      (Included(start), Included(end)) => {
        let range = byte_index(&self.text, *start)..=byte_index(&self.text, *end);
        get(&self.text, range)
      },
      (Included(start), Excluded(end)) => {
        let range = byte_index(&self.text, *start)..byte_index(&self.text, *end);
        get(&self.text, range)
      },
      (Unbounded, Unbounded) => &self.text,
      (Unbounded, Included(end)) => {
        let end = byte_index(&self.text, *end);
        let range = ..=min(self.text.len().saturating_sub(1), end);
        get(&self.text, range)
      },
      (Unbounded, Excluded(end)) => {
        let range = ..byte_index(&self.text, *end);
        get(&self.text, range)
      },
      _ => unimplemented!(),
    }
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

  /// Convert the object into a `String`.
  #[inline]
  pub fn into_string(self) -> String {
    self.text
  }
}

impl Deref for Text {
  type Target = str;

  #[inline]
  fn deref(&self) -> &Self::Target {
    self.text.as_str()
  }
}


/// A text with an associated cursor.
///
/// Please note that at the moment, selections take into account
/// character width. That is arguably more of a property pertaining the
/// specific output in use, and so we are effectively specific to
/// terminal based use cases at the moment.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EditableText {
  /// The text.
  text: Text,
  /// The "character" index of the cursor.
  cursor: usize,
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
      text: Text::from_string(text),
      // An index of zero is always valid, even if `text` was empty.
      cursor: 0,
    }
  }

  /// Select the first character.
  pub fn move_start(&mut self) {
    self.cursor = 0;
  }

  /// Move the cursor to the end of the text, i.e., past the
  /// last character.
  pub fn move_end(&mut self) {
    self.cursor = self.len();
  }

  /// Select the next character, if any.
  pub fn move_next(&mut self) {
    self.cursor = min(self.cursor + 1, self.len());
  }

  /// Select the previous character, if any.
  pub fn move_prev(&mut self) {
    self.cursor = min(self.cursor.saturating_sub(1), self.len());
  }

  /// Select a character based on its byte index.
  #[cfg(feature = "readline")]
  pub fn set_cursor_byte_index(&mut self, byte_index: usize) {
    self.cursor = char_index(&self.text, byte_index);
  }

  /// Insert a character into the text at the current cursor position.
  pub fn insert_char(&mut self, c: char) {
    let byte_index = byte_index(&self.text, self.cursor);
    let () = self.text.text.insert(byte_index, c);
    self.cursor = min(self.cursor + 1, self.len());
  }

  /// Remove the currently selected character from the text.
  pub fn remove_char(&mut self) {
    if self.cursor >= self.len() {
      return
    }

    let byte_index = byte_index(&self.text, self.cursor);
    let _removed = self.text.text.remove(byte_index);
    self.cursor = min(self.cursor, self.len());
  }

  /// Retrieve the current cursor index.
  #[inline]
  pub fn cursor(&self) -> usize {
    self.cursor
  }

  /// Retrieve the current cursor position expressed as a byte index.
  #[inline]
  #[cfg(feature = "readline")]
  pub fn cursor_byte_index(&self) -> usize {
    byte_index(&self.text, self.cursor)
  }

  /// Convert the object into a `String`, discarding any cursor
  /// information.
  #[inline]
  pub fn into_string(self) -> String {
    self.text.into_string()
  }
}

impl Deref for EditableText {
  type Target = Text;

  #[inline]
  fn deref(&self) -> &Self::Target {
    &self.text
  }
}

impl DerefMut for EditableText {
  #[inline]
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.text
  }
}


#[cfg(test)]
mod tests {
  use super::*;


  /// Check that our byte indexing works as it should.
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
    assert_eq!(byte_index(s, 3), 5);
    assert_eq!(byte_index(s, 4), 5);
    assert_eq!(byte_index(s, 5), 5);

    let s = "a\nb";
    assert_eq!(byte_index(s, 0), 0);
    assert_eq!(byte_index(s, 1), 1);
    assert_eq!(byte_index(s, 2), 2);
    assert_eq!(byte_index(s, 3), 3);
    assert_eq!(byte_index(s, 4), 3);
    assert_eq!(byte_index(s, 5), 3);
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
    let text = Text::default();
    assert_eq!(text.substr(..), "");
    assert_eq!(text.substr(0..), "");
    assert_eq!(text.substr(1..), "");
    assert_eq!(text.substr(2..), "");

    let text = Text::from_string("s");
    assert_eq!(text.substr(..), "s");
    assert_eq!(text.substr(0..), "s");
    assert_eq!(text.substr(1..), "");
    assert_eq!(text.substr(2..), "");

    let text = Text::from_string("string");
    assert_eq!(text.substr(..), "string");
    assert_eq!(text.substr(0..), "string");
    assert_eq!(text.substr(1..), "tring");
    assert_eq!(text.substr(2..), "ring");
    assert_eq!(text.substr(5..), "g");
    assert_eq!(text.substr(6..), "");
    assert_eq!(text.substr(..0), "");
    assert_eq!(text.substr(..1), "s");
    assert_eq!(text.substr(..2), "st");
    assert_eq!(text.substr(..5), "strin");
    assert_eq!(text.substr(..6), "string");
    assert_eq!(text.substr(..7), "string");
    assert_eq!(text.substr(..8), "string");
    assert_eq!(text.substr(..=0), "s");
    assert_eq!(text.substr(..=1), "st");
    assert_eq!(text.substr(..=2), "str");
    assert_eq!(text.substr(..=5), "string");
    assert_eq!(text.substr(..=6), "string");
    assert_eq!(text.substr(0..0), "");
    assert_eq!(text.substr(0..1), "s");
    assert_eq!(text.substr(0..=1), "st");
    assert_eq!(text.substr(1..1), "");
    assert_eq!(text.substr(1..2), "t");
    assert_eq!(text.substr(1..=2), "tr");
  }

  /// Check that `Text::len` works as expected.
  #[test]
  fn text_length() {
    let text = Text::default();
    assert_eq!(text.len(), 0);

    let text = Text::from_string("⚠️attn⚠️");
    assert_eq!(text.len(), 6);
  }
}
