// *************************************************************************
// * Copyright (C) 2018-2021 Daniel Mueller (deso@posteo.net)              *
// *                                                                       *
// * This program is free software: you can redistribute it and/or modify  *
// * it under the terms of the GNU General Public License as published by  *
// * the Free Software Foundation, either version 3 of the License, or     *
// * (at your option) any later version.                                   *
// *                                                                       *
// * This program is distributed in the hope that it will be useful,       *
// * but WITHOUT ANY WARRANTY; without even the implied warranty of        *
// * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the         *
// * GNU General Public License for more details.                          *
// *                                                                       *
// * You should have received a copy of the GNU General Public License     *
// * along with this program.  If not, see <http://www.gnu.org/licenses/>. *
// *************************************************************************

use gui::Mergeable;


/// A key as used by the UI.
pub use termion::event::Key;


/// An event as used by the UI.
#[derive(Clone, Debug)]
pub enum Event {
  /// An indication that some component changed and that we should
  /// re-render everything.
  Updated,
  /// An indication that the application should quit.
  Quit,
  /// A key press.
  #[cfg(not(feature = "readline"))]
  Key(Key, ()),
  #[cfg(feature = "readline")]
  Key(Key, Vec<u8>),
}

#[cfg(test)]
impl Event {
  pub fn is_updated(&self) -> bool {
    match self {
      Self::Updated => true,
      _ => false,
    }
  }
}

impl From<u8> for Event {
  fn from(b: u8) -> Self {
    #[cfg(not(feature = "readline"))]
    { Event::Key(Key::Char(char::from(b)), ()) }
    #[cfg(feature = "readline")]
    { Event::Key(Key::Char(char::from(b)), vec![b]) }
  }
}

impl Mergeable for Event {
  fn merge_with(self, other: Self) -> Self {
    match (&self, &other) {
      (Self::Key(..), _) | (_, Self::Key(..)) => panic!(
        "Attempting to merge incompatible events: {:?} & {:?}",
        self, other
      ),
      (Self::Updated, Self::Updated) => self,
      (Self::Quit, _) | (_, Self::Quit) => Self::Quit,
    }
  }
}
