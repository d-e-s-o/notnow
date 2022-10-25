// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::hash::Hash;
use std::hash::Hasher;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::ops::Bound;
use std::ops::Deref;
use std::ops::DerefMut;
use std::slice;

use gaps::RangeGappable as _;


/// An ID used by `Db` instances to uniquely identify an item in it.
///
/// An `Id` is guaranteed to be unique per `Db` instance (but not
/// necessarily program-wide).
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


pub trait Idable<T> {
  fn id(&self) -> Id<T>;
}


/// An iterator over the items in a `Db`.
pub type Iter<'t, T> = slice::Iter<'t, T>;


/// An object wrapping an item contained in a `Db`, along with its `Id`,
/// and providing read-only access to the former.
#[derive(Debug)]
pub struct Entry<'db, T>(&'db (Id<T>, T));

impl<T> Entry<'_, T> {
  /// Retrieve the entry's `Id`.
  #[allow(unused)]
  pub fn id(&self) -> Id<T> {
    self.0 .0
  }
}

impl<'db, T> Deref for Entry<'db, T> {
  type Target = T;

  fn deref(&self) -> &'db Self::Target {
    &self.0 .1
  }
}


/// An object wrapping an item contained in a `Db`, along with its `Id`,
/// and providing mutable access to the former.
#[derive(Debug)]
pub struct EntryMut<'db, T>(&'db mut (Id<T>, T));

impl<T> EntryMut<'_, T> {
  /// Retrieve the entry's `Id`.
  #[allow(unused)]
  pub fn id(&self) -> Id<T> {
    self.0 .0
  }
}

impl<T> Deref for EntryMut<'_, T> {
  type Target = T;

  fn deref(&self) -> &Self::Target {
    &self.0 .1
  }
}

impl<T> DerefMut for EntryMut<'_, T> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0 .1
  }
}


/// A database for storing arbitrary but fixed data items.
#[derive(Debug)]
pub struct Db<T> {
  /// The data along with its ID, in a well-defined order.
  data: Vec<(Id<T>, T)>,
  /// The IDs currently in use.
  ids: BTreeSet<usize>,
}

impl<T> Db<T> {
  /// Create a database from the items contained in the provided
  /// iterator.
  pub fn try_from_iter<I, J>(iter: I) -> Result<Self, usize>
  where
    T: Idable<T>,
    I: IntoIterator<IntoIter = J>,
    J: ExactSizeIterator<Item = T>,
  {
    let iter = iter.into_iter();
    // We work with a `HashSet` as opposed to a `BTreeSet` here, because
    // it provides faster access given the potentially large number of
    // checks we use it for. Once we bulk-inserted, a `BTreeSet` is more
    // suitable, because it maintains sorted order which allows us to
    // find unused IDs in it much more easily.
    let ids = HashSet::with_capacity(iter.len());
    let data = Vec::with_capacity(iter.len());
    let (data, ids) = iter
      .into_iter()
      .try_fold((data, ids), |(mut data, mut ids), item| {
        let id = item.id();
        if !ids.insert(id.id.get()) {
          return Err(id.id)
        }

        data.push((id, item));
        Ok((data, ids))
      })?;

    let slf = Self {
      data,
      ids: ids.into_iter().collect(),
    };
    Ok(slf)
  }

  /// Look up an item's index given the item's ID.
  #[inline]
  pub fn find(&self, id: Id<T>) -> Option<usize>
  where
    T: Idable<T>,
  {
    self.data.iter().position(|(_, task)| task.id() == id)
  }

  /// Insert an item into the database at the given `index`.
  #[inline]
  pub fn insert(&mut self, index: usize, item: T)
  where
    T: Idable<T>,
  {
    let id = self.reserve_id(item.id().id.get());
    self.data.insert(index, (id, item))
  }

  /// Insert an item at the end of the database.
  #[inline]
  pub fn push(&mut self, item: T)
  where
    T: Idable<T>,
  {
    let id = self.reserve_id(item.id().id.get());
    self.data.push((id, item))
  }

  /// Remove the item at the provided index.
  #[inline]
  pub fn remove(&mut self, index: usize) -> T
  where
    T: Idable<T>,
  {
    let (id, item) = self.data.remove(index);
    self.free_id(id);
    item
  }

  /// Retrieve an [`Entry`] representing the item at the given index in
  /// the database.
  #[allow(unused)]
  #[inline]
  pub fn get(&self, index: usize) -> Option<Entry<'_, T>> {
    self.data.get(index).map(Entry)
  }

  /// Retrieve an [`EntryMut`] representing the item at the given index
  /// in the database.
  #[inline]
  pub fn get_mut(&mut self, index: usize) -> Option<EntryMut<'_, T>> {
    self.data.get_mut(index).map(EntryMut)
  }

  /// Retrieve an iterator over the items of the database.
  #[inline]
  pub fn iter(&self) -> Iter<'_, (Id<T>, T)> {
    self.data.iter()
  }

  /// Attempt to reserve an identifier in this `Db` instance.
  fn try_reserve_id(&mut self, id: usize) -> Option<Id<T>> {
    let id = NonZeroUsize::new(id)?;
    if !self.ids.insert(id.get()) {
      None
    } else {
      Some(Id::from_unique_id(id))
    }
  }

  /// Reserve the identifier in this `Db` instance.
  ///
  /// # Panics
  /// This method panics if the identifier is already used within the
  /// `Db`.
  fn reserve_id(&mut self, id: usize) -> Id<T> {
    self
      .try_reserve_id(id)
      .unwrap_or_else(|| panic!("ID {id} is already in use"))
  }

  /// Allocate a new `Id`, unique to this `Db` instance.
  #[allow(unused)]
  fn allocate_id(&mut self) -> Id<T> {
    let mut gaps = self.ids.gaps(1..=usize::MAX);
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

    let _inserted = self.ids.insert(id);
    debug_assert!(_inserted, "ID {id} already present");

    // SANITY: `id` will never be zero here, because we start gap
    //         detection above at 1.
    let id = NonZeroUsize::new(id).unwrap();
    Id::from_unique_id(id)
  }

  /// Free the given `Id` for future use.
  fn free_id(&mut self, id: Id<T>) {
    let _removed = self.ids.remove(&id.id.get());
    debug_assert!(_removed, "ID {id} was not allocated");
  }
}
