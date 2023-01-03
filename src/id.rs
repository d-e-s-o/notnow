// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::hash::Hash;
use std::hash::Hasher;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::ops::Bound;

use gaps::RangeGappable as _;


/// An ID used to uniquely identify an item in some defined space.
#[derive(Debug)]
#[repr(transparent)]
pub struct Id<T> {
  /// The unique identifier.
  id: NonZeroUsize,
  /// Phantom data for `T`.
  _phantom: PhantomData<T>,
}

impl<T> Display for Id<T> {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    Display::fmt(&self.id, f)
  }
}

impl<T> Id<T> {
  /// Construct a new `Id` item given the provided identifier.
  ///
  /// `id` is assumed to be unique with respect to the space that it
  /// is used in.
  pub fn from_unique_id(id: NonZeroUsize) -> Self {
    Self {
      id,
      _phantom: PhantomData,
    }
  }

  /// Retrieve the numeric value of the `Id`.
  pub fn get(&self) -> NonZeroUsize {
    self.id
  }
}

impl<T> Clone for Id<T> {
  fn clone(&self) -> Self {
    Self {
      id: self.id,
      _phantom: PhantomData,
    }
  }
}

impl<T> Copy for Id<T> {}

impl<T> PartialOrd<Self> for Id<T> {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    self.id.partial_cmp(&other.id)
  }
}

impl<T> Ord for Id<T> {
  fn cmp(&self, other: &Self) -> Ordering {
    self.id.cmp(&other.id)
  }
}

impl<T> PartialEq<Self> for Id<T> {
  fn eq(&self, other: &Self) -> bool {
    self.id.eq(&other.id)
  }
}

impl<T> Eq for Id<T> {}

impl<T> Hash for Id<T> {
  fn hash<H>(&self, state: &mut H)
  where
    H: Hasher,
  {
    self.id.hash(state)
  }
}


/// A trait for allocating `Id`'s.
pub trait AllocId<T> {
  type Id;
  type Entry<'e>
  where
    Self: 'e;

  /// Allocate an unused `Id`.
  ///
  /// # Panics
  ///
  /// This method panics if the available ID space is exhausted.
  fn allocate_id(&mut self) -> (Self::Id, Self::Entry<'_>);

  /// Attempt to reserve an `Id`.
  fn try_reserve_id(&mut self, id: usize) -> Option<(Self::Id, Self::Entry<'_>)>;

  /// Reserve an `Id` and panic if it is already in use.
  ///
  /// # Panics
  ///
  /// This method panics if the provided `id` is already in use.
  fn reserve_id(&mut self, id: usize) -> (Self::Id, Self::Entry<'_>) {
    self
      .try_reserve_id(id)
      .unwrap_or_else(|| panic!("ID {id} is already in use"))
  }

  /// Free an `Id` allocated earlier.
  fn free_id(&mut self, id: Self::Id);
}

impl<T> AllocId<T> for BTreeSet<usize> {
  type Id = Id<T>;
  type Entry<'e> = ();

  fn allocate_id(&mut self) -> (Self::Id, Self::Entry<'_>) {
    let mut gaps = self.gaps(1..=usize::MAX);
    let gap = gaps.next().expect("available ID space is exhausted");
    let id = match gap {
      (Bound::Included(lower), _) => lower,
      (Bound::Excluded(lower), _) => lower + 1,
      (Bound::Unbounded, _) => {
        // SANITY: We should never hit this case by virtue of the lower
        //         bound we provide.
        unreachable!()
      },
    };

    let _inserted = self.insert(id);
    debug_assert!(_inserted, "ID {id} already present");

    // SANITY: `id` will never be zero here, because we start gap
    //         detection above at 1.
    let id = NonZeroUsize::new(id).unwrap();
    (Id::from_unique_id(id), ())
  }

  fn try_reserve_id(&mut self, id: usize) -> Option<(Self::Id, ())> {
    let id = NonZeroUsize::new(id)?;
    if !self.insert(id.get()) {
      None
    } else {
      Some((Id::from_unique_id(id), ()))
    }
  }

  fn reserve_id(&mut self, id: usize) -> (Self::Id, ()) {
    self
      .try_reserve_id(id)
      .unwrap_or_else(|| panic!("ID {id} is already in use"))
  }

  fn free_id(&mut self, id: Self::Id) {
    let _removed = self.remove(&id.get().get());
    debug_assert!(_removed, "ID {id} was not allocated");
  }
}


#[cfg(test)]
mod tests {
  use super::*;


  /// Check that we can allocate and free `Id`'s in a `BTreeSet`.
  #[test]
  fn allocate_and_free_id_btreeset() {
    let mut set = BTreeSet::<usize>::new();

    assert_eq!(set.get(&1), None);
    let (id1, ()) = AllocId::<()>::allocate_id(&mut set);
    assert_eq!(id1.get().get(), 1);
    assert_eq!(set.get(&1), Some(&1));

    assert_eq!(set.get(&2), None);
    let (id2, ()) = AllocId::<()>::allocate_id(&mut set);
    assert_eq!(id2.get().get(), 2);
    assert_eq!(set.get(&2), Some(&2));

    let () = AllocId::<()>::free_id(&mut set, id1);
    assert_eq!(set.get(&1), None);

    let () = set.free_id(id2);
    assert_eq!(set.get(&2), None);

    assert!(set.is_empty());
  }


  /// Check that we can reserve and free `Id`'s in a `BTreeSet`.
  #[test]
  fn reserve_free_id_btreeset() {
    let mut set = BTreeSet::<usize>::new();

    let (id1, ()) = AllocId::<()>::reserve_id(&mut set, 1);
    assert_eq!(id1.get().get(), 1);
    assert_eq!(set.get(&1), Some(&1));

    assert_eq!(AllocId::<()>::try_reserve_id(&mut set, 1), None);

    let () = AllocId::<()>::free_id(&mut set, id1);
    assert_eq!(set.get(&1), None);

    assert!(set.is_empty());

    let (id1, ()) = AllocId::<()>::try_reserve_id(&mut set, 1).unwrap();
    assert_eq!(id1.get().get(), 1);
    assert_eq!(set.get(&1), Some(&1));

    let () = AllocId::<()>::free_id(&mut set, id1);
    assert_eq!(set.get(&1), None);

    assert!(set.is_empty());
  }
}
