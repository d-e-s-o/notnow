// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeSet;
use std::collections::HashSet;
use std::iter::Map;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::ops::DerefMut;
use std::slice;

use crate::id::AllocId as _;
use crate::id::Id;


/// An iterator over the items in a `Db`.
pub type Iter<'t, T> = Map<slice::Iter<'t, (Id<T>, T)>, fn(&'_ (Id<T>, T)) -> &'_ T>;


/// A trait for comparing two items in a `Db` instance.
pub trait Cmp<T> {
  /// Check whether two items are equal.
  fn eq(lhs: &T, rhs: &T) -> bool;
}


/// A type implementing the [`Cmp`] trait, forwarding to an existing
/// [`Eq`] implementation.
#[derive(Debug)]
pub struct UseDefault;

impl<T> Cmp<T> for UseDefault
where
  T: Eq,
{
  #[inline]
  fn eq(lhs: &T, rhs: &T) -> bool {
    lhs.eq(rhs)
  }
}


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
pub struct Db<T, C = UseDefault> {
  /// The data along with its ID, in a well-defined order.
  data: Vec<(Id<T>, T)>,
  /// The IDs currently in use.
  ids: BTreeSet<usize>,
  /// Phantom data for the comparator to use.
  _comparator: PhantomData<C>,
}

impl<T, C> Db<T, C> {
  /// Create a database from the items contained in the provided
  /// iterator.
  pub fn try_from_iter<I, J>(iter: I) -> Result<Self, usize>
  where
    I: IntoIterator<IntoIter = J>,
    J: ExactSizeIterator<Item = (usize, T)>,
  {
    let iter = iter.into_iter();
    // We work with a `HashSet` as opposed to a `BTreeSet` here, because
    // it provides faster access given the potentially large number of
    // checks we use it for. Once we bulk-inserted, a `BTreeSet` is more
    // suitable, because it maintains sorted order which allows us to
    // find unused IDs in it much more easily.
    let ids = HashSet::with_capacity(iter.len());
    let data = Vec::with_capacity(iter.len());
    let (data, ids) =
      iter
        .into_iter()
        .try_fold((data, ids), |(mut data, mut ids), (id, item)| {
          let id = NonZeroUsize::new(id).ok_or(id)?;
          if !ids.insert(id.get()) {
            return Err(id.get())
          }

          data.push((Id::from_unique_id(id), item));
          Ok((data, ids))
        })?;

    let slf = Self {
      data,
      ids: ids.into_iter().collect(),
      _comparator: PhantomData,
    };
    Ok(slf)
  }

  /// Create a database from an iterator of items, assigning new IDs in
  /// the process.
  #[cfg(test)]
  pub fn from_iter<I, J>(iter: I) -> Self
  where
    I: IntoIterator<IntoIter = J>,
    J: ExactSizeIterator<Item = T>,
  {
    let data = iter
      .into_iter()
      .enumerate()
      .map(|(id, item)| (Id::from_unique_id(NonZeroUsize::new(id + 1).unwrap()), item))
      .collect::<Vec<_>>();
    let ids = data.iter().map(|(id, _)| id.get().get()).collect();

    Self {
      data,
      ids,
      _comparator: PhantomData,
    }
  }

  /// Look up an item's index in the `Db`.
  #[inline]
  pub fn find(&self, item: &T) -> Option<usize>
  where
    C: Cmp<T>,
  {
    self.data.iter().position(|(_, item_)| C::eq(item_, item))
  }

  /// Insert an item into the database at the given `index`.
  ///
  /// If `id` is not [`None`], the item will be assigned the provided
  /// ID.
  ///
  /// # Panics
  /// This method panics if `id` is already used within the `Db`.
  #[inline]
  pub fn insert(&mut self, index: usize, id: Option<Id<T>>, item: T) -> EntryMut<'_, T> {
    let (id, ()) = if let Some(id) = id {
      self.ids.reserve_id(id.get().get())
    } else {
      self.ids.allocate_id()
    };
    let () = self.data.insert(index, (id, item));
    // SANITY: We know we just inserted an item at `index`, so an entry
    //         has to exist.
    self.get_mut(index).unwrap()
  }

  /// Insert an item at the end of the database.
  #[inline]
  pub fn push(&mut self, id: Option<Id<T>>, item: T) -> EntryMut<'_, T> {
    let (id, ()) = if let Some(id) = id {
      self.ids.reserve_id(id.get().get())
    } else {
      self.ids.allocate_id()
    };
    let () = self.data.push((id, item));
    // SANITY: We know we just pushed an item, so a last item has to
    //         exist.
    self.last_mut().unwrap()
  }

  /// Remove the item at the provided index.
  #[inline]
  pub fn remove(&mut self, index: usize) -> (Id<T>, T) {
    let (id, item) = self.data.remove(index);
    let () = self.ids.free_id(id);
    (id, item)
  }

  /// Retrieve an [`Entry`] representing the item at the given index in
  /// the database.
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

  /// Retrieve an [`Entry`] representing the last item in the database.
  #[allow(unused)]
  #[inline]
  pub fn last(&self) -> Option<Entry<'_, T>> {
    self.data.last().map(Entry)
  }

  /// Retrieve an [`EntryMut`] representing the last item in the
  /// database.
  #[inline]
  pub fn last_mut(&mut self) -> Option<EntryMut<'_, T>> {
    self.data.last_mut().map(EntryMut)
  }

  /// Retrieve an iterator over the items of the database.
  #[inline]
  pub fn iter(&self) -> Iter<'_, T> {
    fn map<'t, T>(x: &'t (Id<T>, T)) -> &'t T {
      &x.1
    }

    self.data.iter().map(map as _)
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;


  /// Make sure that we can create a [`Db`] from an iterator.
  #[test]
  fn create_from_iter() {
    let mut items = [(1, "foo"), (2, "bar"), (3, "baz"), (4, "foobar")];

    let db = Db::<_, UseDefault>::try_from_iter(items).unwrap();
    assert_eq!(*db.get(0).unwrap(), "foo");
    assert_eq!(*db.get(3).unwrap(), "foobar");

    // Create duplicate ID in the array.
    items[2].0 = 2;
    let duplicate = Db::<_, UseDefault>::try_from_iter(items).unwrap_err();
    assert_eq!(duplicate, 2);
  }

  /// Check that we can lookup an item.
  #[test]
  fn find_item() {
    let items = [(1, "foo"), (2, "bar"), (3, "baz"), (4, "foobar")];

    let db = Db::<_, UseDefault>::try_from_iter(items).unwrap();
    assert_eq!(db.find(&"bar"), Some(1));

    let db = Db::<_, UseDefault>::try_from_iter(items).unwrap();
    assert_eq!(db.find(&"hihi"), None);
  }

  /// Check that we can insert an item.
  #[test]
  fn insert_item() {
    let items = [(1, "foo"), (2, "bar"), (3, "baz"), (4, "foobar")];

    let mut db = Db::<_, UseDefault>::try_from_iter(items).unwrap();
    let item = *db.insert(0, None, "foobarbaz").deref();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 0);

    let item = *db.insert(5, None, "outoffoos").deref();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 5);
  }

  /// Check that we can insert an item at the end of a `Db`.
  #[test]
  fn push_item() {
    let mut db = Db::<_, UseDefault>::try_from_iter([]).unwrap();
    let item = *db.push(None, "foo").deref();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 0);

    let item = *db.push(None, "bar").deref();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 1);

    let removed = db.remove(0);
    let entry = db.push(Some(removed.0), "baz");
    assert_eq!(entry.id(), removed.0);

    let item = *entry.deref();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 1);
  }

  /// Check that we can mutate items in a [`Db`].
  #[test]
  fn mutate_item() {
    let items = [(1, "foo"), (2, "bar"), (3, "baz"), (4, "foobar")];

    let mut db = Db::<_, UseDefault>::try_from_iter(items).unwrap();
    let mut entry = db.get_mut(0).unwrap();
    *entry = "bedazzle";

    assert_eq!(*db.get(0).unwrap(), "bedazzle");
  }

  /// Check that we can iterate over the elements of a [`Db`].
  #[test]
  fn iteration() {
    let items = [(1, "foo"), (2, "bar"), (3, "baz"), (4, "foobar")];

    let db = Db::<_, UseDefault>::try_from_iter(items).unwrap();
    let vec = db
      .iter()
      .enumerate()
      .map(|(id, item)| (id + 1, *item))
      .collect::<Vec<_>>();
    assert_eq!(vec, items);
  }
}
