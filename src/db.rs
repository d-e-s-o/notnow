// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

#[cfg(test)]
use std::collections::HashSet;
use std::ops::Deref;
use std::rc::Rc;
use std::slice;


/// An iterator over the items in a `Db`.
pub type Iter<'db, T> = slice::Iter<'db, Rc<T>>;


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


/// A database for storing arbitrary data items.
///
/// Data is stored in reference-counted, heap-allocated manner using
/// [`Rc`]. The database ensures that each item is unique, meaning that
/// it prevents insertion of the same `Rc` instance multiple times (but
/// it does not make any claims about the uniqueness of the inner `T`).
#[derive(Debug)]
pub struct Db<T> {
  /// The data this database manages, in a well-defined order.
  data: Vec<Rc<T>>,
}

impl<T> Db<T> {
  /// Create a database from the items contained in the provided
  /// iterator.
  #[cfg(test)]
  pub fn try_from_iter<I>(iter: I) -> Result<Self, Rc<T>>
  where
    I: IntoIterator<Item = Rc<T>>,
  {
    let data = iter.into_iter().collect::<Vec<_>>();
    // Check that all pointers provided are unique.
    let set = HashSet::with_capacity(data.len());
    let _set = data.iter().try_fold(set, |mut set, rc| {
      if !set.insert(Rc::as_ptr(rc)) {
        Err(rc.clone())
      } else {
        Ok(set)
      }
    })?;

    let slf = Self { data };
    Ok(slf)
  }

  /// Create a database from an iterator of items.
  pub fn from_iter<I>(iter: I) -> Self
  where
    I: IntoIterator<Item = T>,
  {
    Self {
      data: iter.into_iter().map(Rc::new).collect(),
    }
  }

  /// Look up an item's index in the `Db`.
  #[inline]
  pub fn find(&self, item: &Rc<T>) -> Option<usize> {
    self.data.iter().position(|item_| Rc::ptr_eq(item_, item))
  }

  /// Insert an item into the database at the given `index`.
  #[cfg(test)]
  #[inline]
  pub fn insert(&mut self, index: usize, item: T) -> Entry<'_, Rc<T>> {
    let () = self.data.insert(index, Rc::new(item));
    // SANITY: We know we just inserted an item at `index`, so an entry
    //         has to exist.
    self.get(index).unwrap()
  }

  /// Try inserting an item into the database at the given `index`.
  ///
  /// This function succeeds if `item` is not yet present.
  #[inline]
  pub fn try_insert(&mut self, index: usize, item: Rc<T>) -> Option<Entry<'_, Rc<T>>> {
    if self.find(&item).is_some() {
      None
    } else {
      let () = self.data.insert(index, item);
      self.get(index)
    }
  }

  /// Insert an item at the end of the database.
  #[cfg(test)]
  #[inline]
  pub fn push(&mut self, item: T) -> Entry<'_, Rc<T>> {
    let () = self.data.push(Rc::new(item));
    // SANITY: We know we just pushed an item, so a last item has to
    //         exist.
    self.last().unwrap()
  }

  /// Try inserting an item at the end of the database.
  ///
  /// This function succeeds if `item` is not yet present.
  #[inline]
  pub fn try_push(&mut self, item: Rc<T>) -> Option<Entry<'_, Rc<T>>> {
    if self.find(&item).is_some() {
      None
    } else {
      let () = self.data.push(item);
      self.last()
    }
  }

  /// Remove the item at the provided index.
  #[inline]
  pub fn remove(&mut self, index: usize) -> Rc<T> {
    self.data.remove(index)
  }

  /// Retrieve an [`Entry`] representing the item at the given index in
  /// the database.
  #[inline]
  pub fn get(&self, index: usize) -> Option<Entry<'_, Rc<T>>> {
    self.data.get(index).map(Entry)
  }

  /// Retrieve an [`Entry`] representing the last item in the database.
  #[inline]
  pub fn last(&self) -> Option<Entry<'_, Rc<T>>> {
    self.data.last().map(Entry)
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
    let db = Db::from_iter(items);
    assert_eq!(**db.get(0).unwrap(), "foo");
    assert_eq!(**db.get(3).unwrap(), "foobar");
  }

  /// Make sure that [`Db`] creation fails if duplicate items are
  /// provided.
  #[test]
  fn create_from_iter_duplicate() {
    let foo = Rc::new("foo");
    let items = [
      foo.clone(),
      Rc::new("bar"),
      Rc::new("baz"),
      foo.clone(),
      Rc::new("foobar"),
    ];
    let duplicate = Db::try_from_iter(items).unwrap_err();
    assert!(Rc::ptr_eq(&duplicate, &foo));
  }

  /// Check that we can lookup an item.
  #[test]
  fn find_item() {
    let items = ["foo", "bar", "baz", "foobar"]
      .into_iter()
      .map(Rc::new)
      .collect::<Vec<_>>();
    let bar = items[1].clone();

    let db = Db::try_from_iter(items.clone()).unwrap();
    assert_eq!(db.find(&bar), Some(1));

    let hihi = Rc::new("hihi");
    let db = Db::try_from_iter(items).unwrap();
    assert_eq!(db.find(&hihi), None);
  }

  /// Check that we can insert an item.
  #[test]
  fn insert_item() {
    let items = ["foo", "bar", "baz", "foobar"];

    let mut db = Db::from_iter(items);
    let item = db.insert(0, "foobarbaz").deref().clone();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 0);

    let item = db.insert(5, "outoffoos").deref().clone();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 5);
  }

  /// Check that we can insert an item, but fail if it is a duplicate.
  #[test]
  fn try_insert_item() {
    let items = ["foo", "bar", "baz", "foobar"];

    let mut db = Db::from_iter(items);
    let item = db
      .try_insert(0, Rc::new("foobarbaz"))
      .unwrap()
      .deref()
      .clone();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 0);

    let item = db.get(0).unwrap().deref().clone();
    assert!(db.try_insert(5, item).is_none())
  }

  /// Check that we can insert an item at the end of a `Db`.
  #[test]
  fn push_item() {
    let mut db = Db::from_iter([]);
    let item = db.push("foo").deref().clone();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 0);

    let item = db.push("bar").deref().clone();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 1);

    let _removed = db.remove(0);
    let item = db.push("baz").deref().clone();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 1);
  }

  /// Check that we can insert an item at the end of a `Db`, but fail if
  /// it is a duplicate.
  #[test]
  fn try_push_item() {
    let mut db = Db::from_iter(["foo", "boo", "blah"]);
    let item = db.try_push(Rc::new("foo")).unwrap().deref().clone();
    let idx = db.find(&item).unwrap();
    assert_eq!(idx, 3);

    let item = db.get(1).unwrap().deref().clone();
    assert!(db.try_push(item).is_none())
  }

  /// Check that we can iterate over the elements of a [`Db`].
  #[test]
  fn iteration() {
    let items = ["foo", "bar", "baz", "foobar"];

    let db = Db::from_iter(items);
    let vec = db.iter().map(|rc| **rc).collect::<Vec<_>>();
    assert_eq!(vec, items);
  }
}
