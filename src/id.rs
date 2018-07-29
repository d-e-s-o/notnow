// id.rs

// *************************************************************************
// * Copyright (C) 2018 Daniel Mueller (deso@posteo.net)                   *
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

use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result;
use std::marker::PhantomData;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use ser::id::Id as SerId;


/// A struct representing IDs usable for various purposes.
///
/// Note that for some reason Rust only truly provides the
/// implementations of the various traits we derive from when `T` also
/// provides them. Note furthermore that we want all ID objects to be
/// lightweight and, hence, require the implementation of `Copy` for `T`
/// (which we do not for all the other, optional, traits).
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Id<T>
where
  T: Copy,
{
  id: usize,
  phantom: PhantomData<T>,
}

impl<T> Id<T>
where
  T: Copy,
{
  /// Create a new unique `Id`.
  pub fn new() -> Self {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    Id {
      id: id,
      phantom: PhantomData,
    }
  }

  /// Convert this `Id` into a serializable one.
  ///
  /// Note that it is generally safe to convert this unique in-memory ID
  /// into a serializable one. However, the inverse conversion is not
  /// allowed, for there is no way to guarantee uniqueness of the
  /// resulting in-memory ID.
  pub fn to_serde<U>(self) -> SerId<U>
  where
    U: Copy,
  {
    SerId::new(self.id)
  }
}

impl<T> Debug for Id<T>
where
  T: Copy,
{
  fn fmt(&self, f: &mut Formatter) -> Result {
    write!(f, "Id {{ id: {} }}", self.id)
  }
}

impl<T> Display for Id<T>
where
  T: Copy,
{
  /// Format the `Id` into the given formatter.
  fn fmt(&self, f: &mut Formatter) -> Result {
    write!(f, "{}", self.id)
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  type TestId = Id<u32>;


  #[test]
  fn unique_id_increases() {
    let id1 = TestId::new();
    let id2 = TestId::new();

    assert!(id2.id > id1.id);
  }
}
