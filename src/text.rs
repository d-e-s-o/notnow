// Copyright (C) 2023-2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! Please note that we use the word character ("char") loosely in this
//! module, referring to what a user would intuitively describe as a
//! character. Really it's a grapheme cluster in Unicode speak. All
//! indexes, unless explicitly denoted otherwise, are relative to these
//! characters and not to bytes.

use std::cmp::min;
use std::ops::Add;
use std::ops::AddAssign;
use std::ops::Bound::Excluded;
use std::ops::Bound::Included;
use std::ops::Bound::Unbounded;
use std::ops::ControlFlow;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ops::RangeBounds;
use std::ops::Sub;
use std::ops::SubAssign;
use std::slice::SliceIndex;

use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthChar as _;
use unicode_width::UnicodeWidthStr as _;


/// A type representing the width of a string (in "columns"), as it
/// would be displayed.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Width(usize);

impl From<usize> for Width {
  #[inline]
  fn from(other: usize) -> Self {
    Width(other)
  }
}

impl Add<Width> for Width {
  type Output = Width;

  #[inline]
  fn add(mut self, other: Width) -> Self::Output {
    self += other;
    self
  }
}

impl AddAssign<Width> for Width {
  #[inline]
  fn add_assign(&mut self, other: Width) {
    self.0 += other.0;
  }
}

impl Sub<Width> for Width {
  type Output = Width;

  #[inline]
  fn sub(mut self, other: Width) -> Self::Output {
    self -= other;
    self
  }
}

impl SubAssign<Width> for Width {
  #[inline]
  fn sub_assign(&mut self, other: Width) {
    self.0 = self.0.saturating_sub(other.0);
  }
}


/// A trait for conveniently querying the width of an entity.
pub trait DisplayWidth {
  fn display_width(&self) -> Width;
}

impl DisplayWidth for str {
  fn display_width(&self) -> Width {
    let extended = true;
    let cursor = self
      .graphemes(extended)
      .map(|grapheme| grapheme.width())
      .sum();

    Width(cursor)
  }
}

impl DisplayWidth for char {
  fn display_width(&self) -> Width {
    Width(self.width().unwrap_or(0))
  }
}


/// A cursor into a string (most likely actually an [`EditableText`] to
/// be used mainly for display purposes.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cursor(usize);

impl Cursor {
  /// Create a [`Cursor`] object pointing at the beginning of a string.
  ///
  /// # Notes
  /// This method constitutes the only public constructor for this type.
  pub fn at_start() -> Cursor {
    Self(0)
  }

  /// Convert the object back into a [`usize`].
  pub fn as_usize(&self) -> usize {
    self.0
  }
}

impl Add<Width> for Cursor {
  type Output = Cursor;

  #[inline]
  fn add(mut self, other: Width) -> Self::Output {
    self += other;
    self
  }
}

impl AddAssign<Width> for Cursor {
  #[inline]
  fn add_assign(&mut self, other: Width) {
    self.0 += other.0;
  }
}

impl Sub<Width> for Cursor {
  type Output = Cursor;

  #[inline]
  fn sub(mut self, other: Width) -> Self::Output {
    self -= other;
    self
  }
}

impl SubAssign<Width> for Cursor {
  #[inline]
  fn sub_assign(&mut self, other: Width) {
    self.0 = self.0.saturating_sub(other.0);
  }
}


/// Find the byte index that maps to the given [`Cursor`] position.
fn cursor_byte_index(string: &str, cursor: Cursor) -> usize {
  let extended = true;
  let result = string.grapheme_indices(extended).try_fold(
    Width::from(0),
    |total_width, (byte_idx, grapheme)| {
      if Cursor::at_start() + total_width >= cursor {
        ControlFlow::Break(byte_idx)
      } else {
        ControlFlow::Continue(total_width + grapheme.display_width())
      }
    },
  );

  match result {
    ControlFlow::Break(byte_idx) => byte_idx,
    ControlFlow::Continue(_) => string.len(),
  }
}

/// Find the byte index that maps to the given character position.
fn char_byte_index(string: &str, char_idx: usize) -> usize {
  let extended = true;
  let result = string.grapheme_indices(extended).enumerate().try_for_each(
    |(char_pos, (byte_idx, _grapheme))| {
      if char_pos >= char_idx {
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
fn char_index(string: &str, byte_idx: usize) -> usize {
  let extended = true;
  string
    .grapheme_indices(extended)
    .map_while(|(idx, grapheme)| {
      if byte_idx >= idx + grapheme.len() {
        Some(())
      } else {
        None
      }
    })
    .count()
}


/// Find the cursor index that maps to the given byte position.
fn cursor_index(string: &str, byte_idx: usize) -> Cursor {
  let extended = true;
  let width = string
    .grapheme_indices(extended)
    .map_while(|(idx, grapheme)| {
      if byte_idx >= idx + grapheme.len() {
        Some(grapheme.display_width())
      } else {
        None
      }
    })
    .reduce(Width::add);

  Cursor::at_start() + width.unwrap_or(Width::from(0))
}


/// Clip a string at `max_width` characters, at a proper character
/// boundary.
pub(crate) fn clip(string: &str, max_width: Width) -> &str {
  let extended = true;
  let result = string.grapheme_indices(extended).try_fold(
    Width::from(0),
    |mut total_width, (byte_idx, grapheme)| {
      total_width += grapheme.display_width();
      if total_width > max_width {
        ControlFlow::Break(byte_idx)
      } else {
        ControlFlow::Continue(total_width)
      }
    },
  );

  match result {
    ControlFlow::Break(byte_idx) => &string[..byte_idx],
    ControlFlow::Continue(..) => string,
  }
}


/// Wrap `string` to at most [`max_width`] characters, at a word
/// boundary.
///
/// # Returns
/// This function returns a tuple of:
/// - the potentially split input string
/// - the optional remainder of `string` that did not fit into
///   `max_width`
///
/// # Notes
/// - if `string` contains only a single word that exceeds
///   [`max_width`], the returned string will be split at a non-word
///   boundary to honor the width
/// - newlines are not treated any specially; in all likelihood you want
///   to consider newline *before* attempting any kind of wrapping
/// - only a single wrap is performed; the remainder may exceed
///   [`max_width`] characters
pub(crate) fn wrap(string: &str, max_width: Width) -> (&str, Option<&str>) {
  fn report(string: &str, byte_idx: usize) -> (&str, Option<&str>) {
    if byte_idx == string.len() {
      (string, None)
    } else {
      let (string, remainder) = string.split_at(byte_idx);
      (string, Some(remainder))
    }
  }

  debug_assert_ne!(max_width, Width::from(0));

  let mut words = string.unicode_word_indices();
  let result = words.try_fold(
    (0, Width::from(0)),
    |(last_byte_idx, last_total_width), (byte_idx, word)| {
      let total_width = last_total_width + string[last_byte_idx..byte_idx].display_width();
      if total_width > max_width {
        ControlFlow::Break((last_byte_idx, last_total_width, byte_idx))
      } else {
        let word_width = word.display_width();
        if total_width + word_width > max_width {
          if last_byte_idx != 0 {
            ControlFlow::Break((byte_idx, total_width, byte_idx))
          } else {
            ControlFlow::Break((byte_idx, total_width, byte_idx + word.len()))
          }
        } else {
          ControlFlow::Continue((byte_idx + word.len(), total_width + word_width))
        }
      }
    },
  );

  let (last_byte_idx, last_total_width, byte_idx) = match result {
    ControlFlow::Break((last_byte_idx, last_total_width, byte_idx)) => {
      debug_assert!(
        last_total_width <= max_width,
        "{last_total_width:?} : {max_width:?}"
      );
      (last_byte_idx, last_total_width, byte_idx)
    },
    ControlFlow::Continue((byte_idx, total_width)) => {
      debug_assert!(total_width <= max_width, "{total_width:?} : {max_width:?}");
      // We ran out of words but haven't reached the width limit yet.
      // That being said, we only iterated over actual words, but there
      // may be additional non-word characters that we may have to cut
      // off.
      (byte_idx, total_width, string.len())
    },
  };

  let non_words = &string[last_byte_idx..byte_idx];
  let clipped = clip(non_words, max_width - last_total_width);
  report(string, last_byte_idx + clipped.len())
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
    R: RangeBounds<Cursor>,
  {
    fn get(text: &str, range: impl SliceIndex<str, Output = str>) -> &str {
      text.get(range).unwrap_or("")
    }

    let start = range.start_bound();
    let end = range.end_bound();

    match (start, end) {
      (Included(start), Unbounded) => {
        let range = cursor_byte_index(&self.text, *start)..;
        get(&self.text, range)
      },
      (Included(start), Included(end)) => {
        let range = cursor_byte_index(&self.text, *start)..=cursor_byte_index(&self.text, *end);
        get(&self.text, range)
      },
      (Included(start), Excluded(end)) => {
        let range = cursor_byte_index(&self.text, *start)..cursor_byte_index(&self.text, *end);
        get(&self.text, range)
      },
      (Unbounded, Unbounded) => &self.text,
      (Unbounded, Included(end)) => {
        let end = cursor_byte_index(&self.text, *end);
        let range = ..=min(self.text.len().saturating_sub(1), end);
        get(&self.text, range)
      },
      (Unbounded, Excluded(end)) => {
        let range = ..cursor_byte_index(&self.text, *end);
        get(&self.text, range)
      },
      _ => unimplemented!(),
    }
  }

  /// Retrieve the number of characters in the text.
  #[inline]
  pub fn char_count(&self) -> usize {
    self.text.graphemes(true).count()
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
  char_idx: usize,
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
      char_idx: 0,
    }
  }

  /// Retrieve a [`Cursor`] object pointing at the very first character
  /// in the text (or, if the text is empty, past the last one).
  pub fn cursor_start(&self) -> Cursor {
    Cursor::at_start()
  }

  /// Retrieve a [`Cursor`] object pointing past the last character of
  /// the text.
  pub fn cursor_end(&self) -> Cursor {
    cursor_index(&self.text.text, self.text.text.len())
  }

  /// Select the first character.
  pub fn move_start(&mut self) {
    self.char_idx = 0;
  }

  /// Move the cursor to the end of the text, i.e., past the
  /// last character.
  pub fn move_end(&mut self) {
    self.char_idx = self.char_count();
  }

  /// Select the next character, if any.
  pub fn move_next(&mut self) {
    self.char_idx = min(self.char_idx + 1, self.char_count());
  }

  /// Select the previous character, if any.
  pub fn move_prev(&mut self) {
    self.char_idx = min(self.char_idx.saturating_sub(1), self.char_count());
  }

  /// Insert a character into the text at the current cursor position.
  ///
  /// # Notes
  /// Strictly speaking a [`char`] input is insufficient here: a [`str`]
  /// is more appropriate as it can represent a grapheme cluster.
  /// A [`char`] is enough given the current set of clients we have,
  /// though.
  pub fn insert_char(&mut self, c: char) {
    let byte_index = char_byte_index(&self.text.text, self.char_idx);
    let () = self.text.text.insert(byte_index, c);
    let () = self.move_next();
  }

  /// Remove the currently selected character from the text.
  pub fn remove_char(&mut self) {
    if self.char_idx >= self.char_count() {
      return
    }

    let byte_idx_start = char_byte_index(&self.text.text, self.char_idx);
    let byte_idx_end = char_byte_index(&self.text.text, self.char_idx + 1);

    let () = self
      .text
      .text
      .replace_range(byte_idx_start..byte_idx_end, "");
    self.char_idx = min(self.char_idx, self.char_count());
  }

  /// Retrieve the current cursor index.
  #[inline]
  pub fn cursor(&self) -> Cursor {
    let byte_idx = char_byte_index(&self.text.text, self.char_idx);
    cursor_index(&self.text.text, byte_idx)
  }

  /// Retrieve the current cursor position expressed as a byte index.
  #[inline]
  pub fn cursor_byte_index(&self) -> usize {
    char_byte_index(&self.text.text, self.char_idx)
  }

  /// Select a character based on its byte index.
  #[inline]
  pub fn set_cursor_byte_index(&mut self, byte_index: usize) {
    self.char_idx = char_index(&self.text.text, byte_index);
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


  /// A *test-only* helper for quick [`Cursor`] creation.
  fn c(cursor: usize) -> Cursor {
    Cursor(cursor)
  }

  /// A *test-only* helper for quick [`Width`] creation.
  fn w(cursor: usize) -> Width {
    Width(cursor)
  }

  /// Check that our byte indexing works as it should.
  #[test]
  fn byte_indexing() {
    let s = "";
    assert_eq!(cursor_byte_index(s, c(0)), 0);
    assert_eq!(cursor_byte_index(s, c(1)), 0);

    let s = "s";
    assert_eq!(cursor_byte_index(s, c(0)), 0);
    assert_eq!(cursor_byte_index(s, c(1)), 1);
    assert_eq!(cursor_byte_index(s, c(2)), 1);

    let s = "foobar";
    assert_eq!(cursor_byte_index(s, c(0)), 0);
    assert_eq!(cursor_byte_index(s, c(1)), 1);
    assert_eq!(cursor_byte_index(s, c(5)), 5);
    assert_eq!(cursor_byte_index(s, c(6)), 6);
    assert_eq!(cursor_byte_index(s, c(7)), 6);

    let s = "⚠️attn⚠️";
    assert_eq!(cursor_byte_index(s, c(0)), 0);
    assert_eq!(cursor_byte_index(s, c(1)), 6);
    assert_eq!(cursor_byte_index(s, c(2)), 7);
    assert_eq!(cursor_byte_index(s, c(3)), 8);
    assert_eq!(cursor_byte_index(s, c(5)), 10);
    assert_eq!(cursor_byte_index(s, c(6)), 16);
    assert_eq!(cursor_byte_index(s, c(7)), 16);

    let s = "a｜b";
    assert_eq!(cursor_byte_index(s, c(0)), 0);
    assert_eq!(cursor_byte_index(s, c(1)), 1);
    assert_eq!(cursor_byte_index(s, c(2)), 4);
    assert_eq!(cursor_byte_index(s, c(3)), 4);
    assert_eq!(cursor_byte_index(s, c(4)), 5);
    assert_eq!(cursor_byte_index(s, c(5)), 5);
    assert_eq!(cursor_byte_index(s, c(6)), 5);
  }

  /// Check that our cursor indexing works as it should.
  #[test]
  fn cursor_indexing() {
    let s = "";
    assert_eq!(cursor_index(s, 0), c(0));
    assert_eq!(cursor_index(s, 1), c(0));

    let s = "s";
    assert_eq!(cursor_index(s, 0), c(0));
    assert_eq!(cursor_index(s, 1), c(1));
    assert_eq!(cursor_index(s, 2), c(1));

    let s = "foobar";
    assert_eq!(cursor_index(s, 0), c(0));
    assert_eq!(cursor_index(s, 1), c(1));
    assert_eq!(cursor_index(s, 5), c(5));
    assert_eq!(cursor_index(s, 6), c(6));
    assert_eq!(cursor_index(s, 7), c(6));

    let s = "⚠️attn⚠️";
    assert_eq!(cursor_index(s, 0), c(0));
    assert_eq!(cursor_index(s, 1), c(0));
    assert_eq!(cursor_index(s, 6), c(1));
    assert_eq!(cursor_index(s, 7), c(2));

    let s = "a｜b";
    assert_eq!(cursor_index(s, 0), c(0));
    assert_eq!(cursor_index(s, 1), c(1));
    assert_eq!(cursor_index(s, 2), c(1));
    assert_eq!(cursor_index(s, 3), c(1));
    assert_eq!(cursor_index(s, 4), c(3));
    assert_eq!(cursor_index(s, 5), c(4));
    assert_eq!(cursor_index(s, 6), c(4));
  }

  /// Check that we can correctly "clip" a string to a maximum width.
  #[test]
  fn string_clipping() {
    assert_eq!(clip("", w(0)), "");
    assert_eq!(clip("a", w(0)), "");
    assert_eq!(clip("ab", w(0)), "");

    assert_eq!(clip("", w(1)), "");
    assert_eq!(clip("a", w(1)), "a");
    assert_eq!(clip("ab", w(1)), "a");

    assert_eq!(clip("⚠️attn⚠️", w(0)), "");
    assert_eq!(clip("⚠️attn⚠️", w(1)), "⚠️");
    assert_eq!(clip("⚠️attn⚠️", w(2)), "⚠️a");

    assert_eq!(clip("｜a｜b｜", w(0)), "");
    assert_eq!(clip("｜a｜b｜", w(1)), "");
    assert_eq!(clip("｜a｜b｜", w(2)), "｜");
    assert_eq!(clip("｜a｜b｜", w(3)), "｜a");
  }

  #[test]
  #[cfg(debug_assertions)]
  #[should_panic(expected = "assertion `left != right` failed")]
  fn line_wrapping_no_width() {
    let s = "";
    let (_string, _remainder) = wrap(s, w(0));
  }

  /// Check that we can wrap lines at word boundaries as expected.
  #[test]
  fn line_wrapping() {
    let s = "";
    assert_eq!(wrap(s, w(1)), ("", None));

    let s = "two words";
    assert_eq!(wrap(s, w(1)), ("t", Some("wo words")));
    assert_eq!(wrap(s, w(2)), ("tw", Some("o words")));
    assert_eq!(wrap(s, w(3)), ("two", Some(" words")));
    assert_eq!(wrap(s, w(4)), ("two ", Some("words")));
    assert_eq!(wrap(s, w(5)), ("two ", Some("words")));
    assert_eq!(wrap(s, w(8)), ("two ", Some("words")));
    assert_eq!(wrap(s, w(9)), ("two words", None));

    let s = "two words ";
    assert_eq!(wrap(s, w(1)), ("t", Some("wo words ")));
    assert_eq!(wrap(s, w(2)), ("tw", Some("o words ")));
    assert_eq!(wrap(s, w(3)), ("two", Some(" words ")));
    assert_eq!(wrap(s, w(4)), ("two ", Some("words ")));
    assert_eq!(wrap(s, w(5)), ("two ", Some("words ")));
    assert_eq!(wrap(s, w(8)), ("two ", Some("words ")));
    assert_eq!(wrap(s, w(9)), ("two words", Some(" ")));
    assert_eq!(wrap(s, w(10)), ("two words ", None));

    let s = "⚠️one⚠️";
    assert_eq!(wrap(s, w(1)), ("⚠️", Some("one⚠️")));
    assert_eq!(wrap(s, w(2)), ("⚠️o", Some("ne⚠️")));
    assert_eq!(wrap(s, w(3)), ("⚠️on", Some("e⚠️")));
    assert_eq!(wrap(s, w(4)), ("⚠️one", Some("⚠️")));
    assert_eq!(wrap(s, w(5)), ("⚠️one⚠️", None));

    let s = "one             ";
    assert_eq!(wrap(s, w(1)), ("o", Some("ne             ")));
    assert_eq!(wrap(s, w(2)), ("on", Some("e             ")));
    assert_eq!(wrap(s, w(3)), ("one", Some("             ")));
    assert_eq!(wrap(s, w(4)), ("one ", Some("            ")));
    assert_eq!(wrap(s, w(5)), ("one  ", Some("           ")));
    assert_eq!(wrap(s, w(15)), ("one            ", Some(" ")));
    assert_eq!(wrap(s, w(16)), ("one             ", None));

    let s = "⚠️two⚠️ words⚠️";
    assert_eq!(wrap(s, w(1)), ("⚠️", Some("two⚠️ words⚠️")));
    assert_eq!(wrap(s, w(2)), ("⚠️t", Some("wo⚠️ words⚠️")));
    assert_eq!(wrap(s, w(3)), ("⚠️tw", Some("o⚠️ words⚠️")));
    assert_eq!(wrap(s, w(4)), ("⚠️two", Some("⚠️ words⚠️")));
    assert_eq!(wrap(s, w(11)), ("⚠️two⚠️ words", Some("⚠️")));
    assert_eq!(wrap(s, w(12)), ("⚠️two⚠️ words⚠️", None));

    let s = "⚠️two⚠️ words⚠️ ";
    assert_eq!(wrap(s, w(1)), ("⚠️", Some("two⚠️ words⚠️ ")));
    assert_eq!(wrap(s, w(2)), ("⚠️t", Some("wo⚠️ words⚠️ ")));
    assert_eq!(wrap(s, w(3)), ("⚠️tw", Some("o⚠️ words⚠️ ")));
    assert_eq!(wrap(s, w(4)), ("⚠️two", Some("⚠️ words⚠️ ")));
    assert_eq!(wrap(s, w(11)), ("⚠️two⚠️ words", Some("⚠️ ")));
    assert_eq!(wrap(s, w(12)), ("⚠️two⚠️ words⚠️", Some(" ")));
    assert_eq!(wrap(s, w(13)), ("⚠️two⚠️ words⚠️ ", None));

    let s = "two｜words";
    assert_eq!(wrap(s, w(1)), ("t", Some("wo｜words")));
    assert_eq!(wrap(s, w(2)), ("tw", Some("o｜words")));
    assert_eq!(wrap(s, w(3)), ("two", Some("｜words")));
    assert_eq!(wrap(s, w(4)), ("two", Some("｜words")));
    assert_eq!(wrap(s, w(5)), ("two｜", Some("words")));
    assert_eq!(wrap(s, w(9)), ("two｜", Some("words")));
    assert_eq!(wrap(s, w(10)), ("two｜words", None));
  }

  /// Check that `EditableText::substr` behaves as it should.
  #[test]
  fn text_substr() {
    let text = Text::default();
    assert_eq!(text.substr(..), "");
    assert_eq!(text.substr(c(0)..), "");
    assert_eq!(text.substr(c(1)..), "");
    assert_eq!(text.substr(c(2)..), "");

    let text = Text::from_string("s");
    assert_eq!(text.substr(..), "s");
    assert_eq!(text.substr(c(0)..), "s");
    assert_eq!(text.substr(c(1)..), "");
    assert_eq!(text.substr(c(2)..), "");

    let text = Text::from_string("string");
    assert_eq!(text.substr(..), "string");
    assert_eq!(text.substr(c(0)..), "string");
    assert_eq!(text.substr(c(1)..), "tring");
    assert_eq!(text.substr(c(2)..), "ring");
    assert_eq!(text.substr(c(5)..), "g");
    assert_eq!(text.substr(c(6)..), "");
    assert_eq!(text.substr(..c(0)), "");
    assert_eq!(text.substr(..c(1)), "s");
    assert_eq!(text.substr(..c(2)), "st");
    assert_eq!(text.substr(..c(5)), "strin");
    assert_eq!(text.substr(..c(6)), "string");
    assert_eq!(text.substr(..c(7)), "string");
    assert_eq!(text.substr(..c(8)), "string");
    assert_eq!(text.substr(..=c(0)), "s");
    assert_eq!(text.substr(..=c(1)), "st");
    assert_eq!(text.substr(..=c(2)), "str");
    assert_eq!(text.substr(..=c(5)), "string");
    assert_eq!(text.substr(..=c(6)), "string");
    assert_eq!(text.substr(c(0)..c(0)), "");
    assert_eq!(text.substr(c(0)..c(1)), "s");
    assert_eq!(text.substr(c(0)..=c(1)), "st");
    assert_eq!(text.substr(c(1)..c(1)), "");
    assert_eq!(text.substr(c(1)..c(2)), "t");
    assert_eq!(text.substr(c(1)..=c(2)), "tr");
  }

  /// Check that `Text::len` works as expected.
  #[test]
  fn text_length() {
    let text = Text::default();
    assert_eq!(text.char_count(), 0);

    let text = Text::from_string("⚠️attn⚠️");
    assert_eq!(text.char_count(), 6);
  }

  /// Make sure that we can insert characters into a [`EditableText`]
  /// object as expected.
  #[test]
  fn character_insertion() {
    let mut text = EditableText::default();
    let () = text.insert_char('a');
    let () = text.insert_char('b');
    let () = text.insert_char('c');
    assert_eq!(text.as_str(), "abc");

    let mut text = EditableText::default();
    let () = text.insert_char('a');
    let () = text.insert_char('\n');
    let () = text.insert_char('c');
    assert_eq!(text.as_str(), "a\nc");

    let mut text = EditableText::from_string("⚠️attn⚠️");
    let () = text.insert_char('x');
    assert_eq!(text.as_str(), "x⚠️attn⚠️");
    let () = text.move_next();
    let () = text.insert_char('y');
    assert_eq!(text.as_str(), "x⚠️yattn⚠️");
    let () = text.move_end();
    let () = text.insert_char('z');
    assert_eq!(text.as_str(), "x⚠️yattn⚠️z");

    let mut text = EditableText::from_string("⚠️attn⚠️");
    let () = text.insert_char('x');
    assert_eq!(text.as_str(), "x⚠️attn⚠️");
    let () = text.move_next();
    let () = text.insert_char('y');
    assert_eq!(text.as_str(), "x⚠️yattn⚠️");
    let () = text.move_end();
    let () = text.insert_char('z');
    assert_eq!(text.as_str(), "x⚠️yattn⚠️z");
    let () = text.move_start();
    let () = text.remove_char();
    assert_eq!(text.as_str(), "⚠️yattn⚠️z");
    let () = text.insert_char('x');
    assert_eq!(text.as_str(), "x⚠️yattn⚠️z");

    let mut text = EditableText::from_string("｜a｜b｜");
    let () = text.insert_char('x');
    assert_eq!(text.as_str(), "x｜a｜b｜");
    let () = text.insert_char('y');
    assert_eq!(text.as_str(), "xy｜a｜b｜");
    let () = text.move_next();
    let () = text.insert_char('z');
    assert_eq!(text.as_str(), "xy｜za｜b｜");
  }

  /// Make sure that we can remove characters from a [`EditableText`]
  /// object as expected.
  #[test]
  fn character_removal() {
    let mut text = EditableText::from_string("⚠️attn⚠️");
    let () = text.remove_char();
    assert_eq!(text.as_str(), "attn⚠️");

    let () = text.move_end();
    // The cursor is at the end, which means nothing should get removed.
    let () = text.remove_char();
    assert_eq!(text.as_str(), "attn⚠️");

    let () = text.move_prev();
    let () = text.remove_char();
    assert_eq!(text.as_str(), "attn");
  }
}
