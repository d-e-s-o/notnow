// Copyright (C) 2018-2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use gui::Id;
use gui::Mergeable;


/// A key as used by the UI.
pub use termion::event::Key;


/// A type representing a set of `ID` objects, providing operations for
/// merging sets.
#[derive(Clone, Debug, PartialEq)]
pub enum Ids<T = Id> {
  One(T),
  Two(T, T),
  Three(T, T, T),
  /// Any number of IDs.
  ///
  /// Note that the order is undefined.
  Any(Vec<T>),
}

impl<T> Ids<T>
where
  T: Clone,
{
  pub fn merge_with(self, other: Self) -> Self {
    match (self, other) {
      (Ids::One(a), Ids::One(b)) => Ids::Two(a, b),
      (Ids::One(a), Ids::Two(b, c)) => Ids::Three(a, b, c),
      (Ids::One(a), Ids::Three(b, c, d)) => Ids::Any(vec![a, b, c, d]),
      (Ids::One(a), Ids::Any(mut b)) => {
        let () = b.extend_from_slice(&[a]);
        Ids::Any(b)
      },
      (Ids::Two(a, b), Ids::One(c)) => Ids::Three(a, b, c),
      (Ids::Two(a, b), Ids::Two(c, d)) => Ids::Any(vec![a, b, c, d]),
      (Ids::Two(a, b), Ids::Three(c, d, e)) => Ids::Any(vec![a, b, c, d, e]),
      (Ids::Two(a, b), Ids::Any(mut c)) => {
        let () = c.extend_from_slice(&[a, b]);
        Ids::Any(c)
      },
      (Ids::Three(a, b, c), Ids::One(d)) => Ids::Any(vec![a, b, c, d]),
      (Ids::Three(a, b, c), Ids::Two(d, e)) => Ids::Any(vec![a, b, c, d, e]),
      (Ids::Three(a, b, c), Ids::Three(d, e, f)) => Ids::Any(vec![a, b, c, d, e, f]),
      (Ids::Three(a, b, c), Ids::Any(mut d)) => {
        let () = d.extend_from_slice(&[a, b, c]);
        Ids::Any(d)
      },
      (Ids::Any(mut a), Ids::One(b)) => {
        let () = a.extend_from_slice(&[b]);
        Ids::Any(a)
      },
      (Ids::Any(mut a), Ids::Two(b, c)) => {
        let () = a.extend_from_slice(&[b, c]);
        Ids::Any(a)
      },
      (Ids::Any(mut a), Ids::Three(b, c, d)) => {
        let () = a.extend_from_slice(&[b, c, d]);
        Ids::Any(a)
      },
      (Ids::Any(mut a), Ids::Any(mut b)) => {
        let () = a.append(&mut b);
        Ids::Any(a)
      },
    }
  }
}


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
    matches!(self, Self::Updated)
  }
}

impl From<u8> for Event {
  fn from(b: u8) -> Self {
    #[cfg(not(feature = "readline"))]
    {
      Event::Key(Key::Char(char::from(b)), ())
    }
    #[cfg(feature = "readline")]
    {
      Event::Key(Key::Char(char::from(b)), vec![b])
    }
  }
}

impl Mergeable for Event {
  fn merge_with(self, other: Self) -> Self {
    match (&self, &other) {
      (Self::Key(..), _) | (_, Self::Key(..)) => {
        panic!("Attempting to merge incompatible events: {self:?} & {other:?}")
      },
      (Self::Updated, Self::Updated) => self,
      (Self::Quit, _) | (_, Self::Quit) => Self::Quit,
    }
  }
}


#[cfg(test)]
mod tests {
  use super::*;


  /// Check that we can merge two `Ids` objects.
  #[test]
  fn ids_merging() {
    let one1 = Ids::One(1);
    let one2 = Ids::One(2);
    let two = one1.merge_with(one2);
    assert_eq!(two, Ids::Two(1, 2));

    let three = two.merge_with(Ids::One(3));
    assert_eq!(three, Ids::Three(1, 2, 3));

    let any = three.merge_with(Ids::One(4));
    assert_eq!(any, Ids::Any(vec![1, 2, 3, 4]));
  }
}
