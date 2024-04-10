// Copyright (C) 2023-2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

#[cfg(feature = "readline")]
use std::ffi::CString;
use std::ops::Deref;
use std::ops::DerefMut;

#[cfg(feature = "readline")]
use rline::Readline;

use termion::event::Key;

use crate::text::EditableText;


/// The result of the handling of input.
pub enum InputResult {
  /// The input was completed into the provided text.
  Completed(String),
  /// Input was canceled.
  Canceled,
  /// The text was updated but input handling is not yet complete.
  Updated,
  /// Input was handled but nothing changed.
  #[cfg_attr(feature = "readline", allow(dead_code))]
  Unchanged,
}


/// A type driving a [`EditableText`] object with terminal based key
/// input.
///
/// Depending on the crate features enabled, input will transparently be
/// libreadline based or not.
#[derive(Debug, Default)]
pub struct InputText {
  text: EditableText,
  /// A readline object used for input.
  #[cfg(feature = "readline")]
  readline: Readline,
}

impl InputText {
  /// Create a new [`InputText`] object working on the provided
  /// [`EditableText`] instance.
  pub fn new(text: EditableText) -> Self {
    Self {
      #[cfg(feature = "readline")]
      readline: {
        let mut rl = Readline::new();
        let cstr = CString::new(text.as_str()).unwrap();
        let cursor = text.cursor_byte_index();
        let clear_undo = true;
        let () = rl.reset(cstr, cursor, clear_undo);
        rl
      },
      text,
    }
  }

  /// Handle a key press.
  #[cfg(not(feature = "readline"))]
  pub fn handle_key(&mut self, key: Key, _raw: &()) -> InputResult {
    use std::mem::take;

    match key {
      Key::Esc => InputResult::Canceled,
      Key::Char('\n') => {
        let line = take(&mut self.text);
        InputResult::Completed(line.into_string())
      },
      Key::Char(c) => {
        let () = self.text.insert_char(c);
        InputResult::Updated
      },
      Key::Backspace => {
        if self.text.cursor() > 0 {
          let () = self.text.move_prev();
          let () = self.text.remove_char();
          InputResult::Updated
        } else {
          InputResult::Unchanged
        }
      },
      Key::Delete => {
        if self.text.cursor() < self.text.len() {
          let () = self.text.remove_char();
          InputResult::Updated
        } else {
          InputResult::Unchanged
        }
      },
      Key::Left => {
        if self.text.cursor() > 0 {
          let () = self.text.move_prev();
          InputResult::Updated
        } else {
          InputResult::Unchanged
        }
      },
      Key::Right => {
        if self.text.cursor() < self.text.len() {
          let () = self.text.move_next();
          InputResult::Updated
        } else {
          InputResult::Unchanged
        }
      },
      Key::Home => {
        if self.text.cursor() != 0 {
          let () = self.text.move_start();
          InputResult::Updated
        } else {
          InputResult::Unchanged
        }
      },
      Key::End => {
        if self.text.cursor() != self.text.len() {
          let () = self.text.move_end();
          InputResult::Updated
        } else {
          InputResult::Unchanged
        }
      },
      _ => InputResult::Unchanged,
    }
  }

  /// Handle a key press.
  #[cfg(feature = "readline")]
  pub fn handle_key(&mut self, key: Key, raw: &[u8]) -> InputResult {
    match self.readline.feed(raw) {
      Some(line) => InputResult::Completed(line.into_string().unwrap()),
      None => {
        let (s, idx) = self.readline.peek(|s, pos| (s.to_owned(), pos));
        // We treat Esc a little specially. In a vi-mode enabled
        // configuration of libreadline Esc cancels input mode when we
        // are in it, and does nothing otherwise. That is what we are
        // interested in here. So we peek at the index we get and see
        // if it changed (because leaving input mode moves the cursor
        // to the left by one). If nothing changed, then we actually
        // cancel the text input. That is not the nicest logic, but
        // the only way we have found that accomplishes what we want.
        if key == Key::Esc && idx == self.text.cursor_byte_index() {
          // TODO: We have a problem here. What may end up happening
          //       is that we disrupt libreadline's workflow by
          //       effectively canceling what it was doing. If, for
          //       instance, we were in vi-movement-mode and we simply
          //       stop the input process libreadline does not know
          //       about that and will stay in this mode. So next time
          //       we start editing again, we will still be in this
          //       mode. Unfortunately, rline's reset does not deal
          //       with this case (perhaps rightly so). For now, just
          //       create a new `Readline` context and that will take
          //       care of resetting things to the default (which is
          //       input mode).
          self.readline = Readline::new();
          InputResult::Canceled
        } else {
          self.text = EditableText::from_string(s.to_string_lossy());
          let () = self.text.set_cursor_byte_index(idx);
          InputResult::Updated
        }
      },
    }
  }
}

impl Deref for InputText {
  type Target = EditableText;

  fn deref(&self) -> &Self::Target {
    &self.text
  }
}

impl DerefMut for InputText {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.text
  }
}
