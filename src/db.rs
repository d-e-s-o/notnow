// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::marker::PhantomData;
use std::ops::Deref;
use std::ops::DerefMut;
use std::slice;


/// An iterator over the items in a `Db`.
pub type Iter<'t, T> = slice::Iter<'t, T>;


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


/// An object wrapping an item contained in a `Db` and providing
/// read-only access to it.
#[derive(Debug)]
pub struct Entry<'db, T>(&'db T);

impl<'db, T> Deref for Entry<'db, T> {
  type Target = T;

  fn deref(&self) -> &'db Self::Target {
    self.0
  }
}


/// An object wrapping an item contained in a `Db` and providing mutable
/// access to it.
#[derive(Debug)]
pub struct EntryMut<'db, T>(&'db mut T);

impl<T> Deref for EntryMut<'_, T> {
  type Target = T;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl<T> DerefMut for EntryMut<'_, T> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}


/// A database for storing arbitrary but fixed data items.
#[derive(Debug)]
pub struct Db<T, C = UseDefault> {
  /// The data this database manages, in a well-defined order.
  data: Vec<T>,
  /// Phantom data for the comparator to use.
  _comparator: PhantomData<C>,
}

impl<T, C> Db<T, C> {
  /// Create a database from the items contained in the provided
  /// iterator.
  pub fn try_from_iter<I>(iter: I) -> Result<Self, usize>
  where
    I: IntoIterator<Item = T>,
  {
    let slf = Self {
      data: iter.into_iter().collect(),
      _comparator: PhantomData,
    };
    Ok(slf)
  }

  /// Create a database from an iterator of items.
  #[cfg(test)]
  pub fn from_iter<I, J>(iter: I) -> Self
  where
    I: IntoIterator<IntoIter = J>,
    J: ExactSizeIterator<Item = T>,
  {
    Self {
      data: iter.into_iter().collect(),
      _comparator: PhantomData,
    }
  }

  /// Look up an item's index in the `Db`.
  #[inline]
  pub fn find(&self, item: &T) -> Option<usize>
  where
    C: Cmp<T>,
  {
    self.data.iter().position(|item_| C::eq(item_, item))
  }

  /// Insert an item into the database at the given `index`.
  #[inline]
  pub fn insert(&mut self, index: usize, item: T) -> EntryMut<'_, T> {
    let () = self.data.insert(index, item);
    // SANITY: We know we just inserted an item at `index`, so an entry
    //         has to exist.
    self.get_mut(index).unwrap()
  }

  /// Insert an item at the end of the database.
  #[inline]
  pub fn push(&mut self, item: T) -> EntryMut<'_, T> {
    let () = self.data.push(item);
    // SANITY: We know we just pushed an item, so a last item has to
    //         exist.
    self.last_mut().unwrap()
  }

  /// Remove the item at the provided index.
  #[inline]
  pub fn remove(&mut self, index: usize) -> T {
    self.data.remove(index)
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
    self.data.iter()
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;


  /// Make sure that we can create a [`Db`] from an iterator.
  #[test]
  fn create_from_iter() {
    let items = ["foo", "bar", "baz", "foobar"];
    let db = Db::<_, UseDefault>::try_from_iter(items).unwrap();
    assert_eq!(*db.get(0).unwrap(), "foo");
    assert_eq!(*db.get(3).unwrap(), "foobar");
  }

  /// Check that we can lookup an item.
  #[test]
  fn find_item() {
    let items = ["foo", "bar", "baz", "foobar"];

    let db = Db::<_, UseDefault>::try_from_iter(items).unwrap();
    assert_eq!(db.find(&"bar"), Some(1));

    let db = Db::<_, UseDefault>::try_from_iter(items).unwrap();
    assert_eq!(db.find(&"hihi"), None);
  }

  /// Check that we can insert an item.
  #[test]
  fn insert_item() {
    let items = ["foo", "bar", "baz", "foobar"];

    let mut db = Db::<_, UseDefault>::try_from_iter(items).unwrap();
    let item = *db.insert(0, "foobarbaz").deref();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 0);

    let item = *db.insert(5, "outoffoos").deref();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 5);
  }

  /// Check that we can insert an item at the end of a `Db`.
  #[test]
  fn push_item() {
    let mut db = Db::<_, UseDefault>::try_from_iter([]).unwrap();
    let item = *db.push("foo").deref();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 0);

    let item = *db.push("bar").deref();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 1);

    let _removed = db.remove(0);
    let item = *db.push("baz").deref();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 1);
  }

  /// Check that we can mutate items in a [`Db`].
  #[test]
  fn mutate_item() {
    let items = ["foo", "bar", "baz", "foobar"];

    let mut db = Db::<_, UseDefault>::try_from_iter(items).unwrap();
    let mut entry = db.get_mut(0).unwrap();
    *entry = "bedazzle";

    assert_eq!(*db.get(0).unwrap(), "bedazzle");
  }

  /// Check that we can iterate over the elements of a [`Db`].
  #[test]
  fn iteration() {
    let items = ["foo", "bar", "baz", "foobar"];

    let db = Db::<_, UseDefault>::try_from_iter(items).unwrap();
    let vec = db.iter().copied().collect::<Vec<_>>();
    assert_eq!(vec, items);
  }
}
