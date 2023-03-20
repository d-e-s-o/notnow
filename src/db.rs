// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::Cell;
#[cfg(test)]
use std::collections::HashSet;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::iter::Map;
use std::ops::Deref;
use std::rc::Rc;
use std::slice;


/// An iterator over the items in a `Db`.
pub type Iter<'db, T, Aux> =
  Map<slice::Iter<'db, (Rc<T>, Cell<Aux>)>, fn(&'_ (Rc<T>, Cell<Aux>)) -> &'_ Rc<T>>;


/// An object wrapping an item contained in a `Db` and providing
/// read-only access to it and its (optional) auxiliary data.
#[derive(Clone)]
pub struct Entry<'db, T, Aux> {
  /// The `Db`'s data.
  data: &'db [(Rc<T>, Cell<Aux>)],
  /// The index of the item represented by the entry.
  index: usize,
}

impl<'db, T, Aux> Entry<'db, T, Aux> {
  /// Create a new `Entry` object.
  #[inline]
  fn new(data: &'db [(Rc<T>, Cell<Aux>)], index: usize) -> Self {
    Self { data, index }
  }

  /// Retrieve the `Entry` for the item following this one, if any.
  #[cfg(test)]
  #[inline]
  pub fn next(&self) -> Option<Entry<'db, T, Aux>> {
    let index = self.index.checked_add(1)?;

    if index < self.data.len() {
      Some(Entry::new(self.data, index))
    } else {
      None
    }
  }

  /// Retrieve the `Entry` for the item before this one, if any.
  #[cfg(test)]
  #[inline]
  pub fn prev(&self) -> Option<Entry<'db, T, Aux>> {
    if self.index > 0 {
      Some(Entry::new(self.data, self.index - 1))
    } else {
      None
    }
  }

  /// Retrieve the index of the element that this `Entry` object
  /// represents in the associated `Db` instance.
  #[inline]
  pub fn index(&self) -> usize {
    self.index
  }
}

impl<T, Aux> Entry<'_, T, Aux>
where
  Aux: Copy,
{
  /// Retrieve a copy of the auxiliary data associated with this
  /// `Entry`.
  #[cfg(test)]
  #[inline]
  pub fn aux(&self) -> Aux {
    self.data[self.index].1.get()
  }

  /// Set the auxiliary data associated with this `Entry`.
  #[cfg(test)]
  #[inline]
  pub fn set_aux(&self, aux: Aux) {
    let () = self.data[self.index].1.set(aux);
  }
}

impl<'db, T, Aux> Deref for Entry<'db, T, Aux> {
  type Target = Rc<T>;

  fn deref(&self) -> &'db Self::Target {
    &self.data[self.index].0
  }
}

impl<T, Aux> Debug for Entry<'_, T, Aux>
where
  T: Debug,
  Aux: Copy + Debug,
{
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    let Self { data, index } = self;

    f.debug_tuple("Entry").field(&data).field(&index).finish()
  }
}


/// A database for storing arbitrary data items.
///
/// Data is stored in reference-counted, heap-allocated manner using
/// [`Rc`]. The database ensures that each item is unique, meaning that
/// it prevents insertion of the same `Rc` instance multiple times (but
/// it does not make any claims about the uniqueness of the inner `T`).
///
/// Associated with each item is optional auxiliary data, which can be
/// accessed via the `Entry` type.
pub struct Db<T, Aux = ()> {
  /// The data this database manages, along with optional auxiliary
  /// data, in a well-defined order.
  data: Vec<(Rc<T>, Cell<Aux>)>,
}

impl<T> Db<T, ()> {
  /// Create a database from the items contained in the provided
  /// iterator.
  #[cfg(test)]
  pub fn try_from_iter<I>(iter: I) -> Result<Self, Rc<T>>
  where
    I: IntoIterator<Item = Rc<T>>,
  {
    let data = iter
      .into_iter()
      .map(|item| (item, Cell::default()))
      .collect::<Vec<_>>();
    // Check that all pointers provided are unique.
    let set = HashSet::with_capacity(data.len());
    let _set = data.iter().try_fold(set, |mut set, (rc, _aux)| {
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
  #[cfg(test)]
  pub fn from_iter<I>(iter: I) -> Self
  where
    I: IntoIterator<Item = T>,
  {
    Self::from_iter_with_aux(iter.into_iter().map(|item| (item, ())))
  }
}

impl<T, Aux> Db<T, Aux> {
  /// Create a database from an iterator of items.
  pub fn from_iter_with_aux<I>(iter: I) -> Self
  where
    I: IntoIterator<Item = (T, Aux)>,
  {
    Self {
      data: iter
        .into_iter()
        .map(|(item, aux)| (Rc::new(item), Cell::new(aux)))
        .collect(),
    }
  }

  /// Look up an item's `Entry` in the `Db`.
  #[inline]
  pub fn find(&self, item: &Rc<T>) -> Option<Entry<'_, T, Aux>> {
    self
      .data
      .iter()
      .position(|(item_, _aux)| Rc::ptr_eq(item_, item))
      .and_then(|idx| self.get(idx))
  }

  /// Insert an item into the database at the given `index`.
  #[cfg(test)]
  #[inline]
  pub fn insert(&mut self, index: usize, item: T) -> Entry<'_, T, Aux>
  where
    Aux: Default,
  {
    self.insert_with_aux(index, item, Aux::default())
  }

  /// Insert an item into the database at the given `index`, providing
  /// an auxiliary value right away.
  #[cfg(test)]
  #[inline]
  pub fn insert_with_aux(&mut self, index: usize, item: T, aux: Aux) -> Entry<'_, T, Aux> {
    let () = self.data.insert(index, (Rc::new(item), Cell::new(aux)));
    // SANITY: We know we just inserted an item at `index`, so an entry
    //         has to exist.
    self.get(index).unwrap()
  }

  /// Try inserting an item into the database at the given `index`.
  ///
  /// This function succeeds if `item` is not yet present.
  #[inline]
  pub fn try_insert(&mut self, index: usize, item: Rc<T>) -> Option<Entry<'_, T, Aux>>
  where
    Aux: Default,
  {
    self.try_insert_with_aux(index, item, Aux::default())
  }

  /// Try inserting an item into the database at the given `index`,
  /// providing a non-default auxiliary value right away.
  ///
  /// This function succeeds if `item` is not yet present.
  #[inline]
  pub fn try_insert_with_aux(
    &mut self,
    index: usize,
    item: Rc<T>,
    aux: Aux,
  ) -> Option<Entry<'_, T, Aux>> {
    if self.find(&item).is_some() {
      None
    } else {
      let () = self.data.insert(index, (item, Cell::new(aux)));
      self.get(index)
    }
  }

  /// Insert an item at the end of the database.
  #[cfg(test)]
  #[inline]
  pub fn push(&mut self, item: T) -> Entry<'_, T, Aux>
  where
    Aux: Default,
  {
    self.push_with_aux(item, Aux::default())
  }

  /// Insert an item at the end of the database, providing a non-default
  /// auxiliary value right away.
  #[cfg(test)]
  #[inline]
  pub fn push_with_aux(&mut self, item: T, aux: Aux) -> Entry<'_, T, Aux> {
    let () = self.data.push((Rc::new(item), Cell::new(aux)));
    // SANITY: We know we just pushed an item, so a last item has to
    //         exist.
    self.last().unwrap()
  }

  /// Try inserting an item at the end of the database.
  ///
  /// This function succeeds if `item` is not yet present.
  #[cfg(test)]
  #[inline]
  pub fn try_push(&mut self, item: Rc<T>) -> Option<Entry<'_, T, Aux>>
  where
    Aux: Default,
  {
    self.try_push_with_aux(item, Aux::default())
  }

  /// Try inserting an item at the end of the database, providing a
  /// non-default auxiliary value right away.
  ///
  /// This function succeeds if `item` is not yet present.
  #[cfg(test)]
  #[inline]
  pub fn try_push_with_aux(&mut self, item: Rc<T>, aux: Aux) -> Option<Entry<'_, T, Aux>> {
    if self.find(&item).is_some() {
      None
    } else {
      let () = self.data.push((item, Cell::new(aux)));
      self.last()
    }
  }

  /// Remove the item at the provided index.
  #[inline]
  pub fn remove(&mut self, index: usize) -> (Rc<T>, Aux) {
    let (item, aux) = self.data.remove(index);
    (item, aux.into_inner())
  }

  /// Retrieve an [`Entry`] representing the item at the given index in
  /// the database.
  #[inline]
  pub fn get(&self, index: usize) -> Option<Entry<'_, T, Aux>> {
    if index < self.data.len() {
      Some(Entry::new(&self.data, index))
    } else {
      None
    }
  }

  /// Retrieve the number of elements in the database.
  #[inline]
  pub fn len(&self) -> usize {
    self.data.len()
  }

  /// Retrieve an [`Entry`] representing the last item in the database.
  #[cfg(test)]
  #[inline]
  pub fn last(&self) -> Option<Entry<'_, T, Aux>> {
    let len = self.data.len();
    if len > 0 {
      Some(Entry::new(&self.data, len - 1))
    } else {
      None
    }
  }

  /// Retrieve an iterator over the items of the database.
  #[inline]
  pub fn iter(&self) -> Iter<'_, T, Aux> {
    fn map<T, Aux>(x: &(T, Cell<Aux>)) -> &T {
      &x.0
    }

    self.data.iter().map(map as _)
  }
}

impl<T, Aux> Debug for Db<T, Aux>
where
  T: Debug,
  Aux: Copy + Debug,
{
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    let Self { data } = self;
    f.debug_struct("Db").field("data", &data).finish()
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;


  /// Check that we can set and get auxiliary data from an `Entry`.
  #[test]
  fn entry_aux_set_get() {
    let iter = ["foo", "boo", "blah"]
      .into_iter()
      .enumerate()
      .map(|(idx, item)| (item, idx));
    let db = Db::from_iter_with_aux(iter);
    let entry = db.get(1).unwrap();
    assert_eq!(entry.aux(), 1);

    let () = entry.set_aux(42);
    assert_eq!(entry.aux(), 42);

    let entry = db.get(1).unwrap();
    assert_eq!(entry.aux(), 42);
  }

  /// Check that `Entry::next` and `Entry::prev` work as they should.
  #[test]
  fn entry_navigation() {
    let db = Db::from_iter(["foo", "boo", "blah"]);

    let entry = db.get(0).unwrap();
    assert_eq!(entry.deref().deref(), &"foo");
    assert!(entry.prev().is_none());

    let entry = entry.next().unwrap();
    assert_eq!(entry.deref().deref(), &"boo");

    let entry = entry.next().unwrap();
    assert_eq!(entry.deref().deref(), &"blah");

    assert!(entry.next().is_none());

    let entry = entry.prev().unwrap();
    assert_eq!(entry.deref().deref(), &"boo");
  }

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
    assert_eq!(db.find(&bar).map(|entry| entry.index()), Some(1));

    let hihi = Rc::new("hihi");
    let db = Db::try_from_iter(items).unwrap();
    assert_eq!(db.find(&hihi).map(|entry| entry.index()), None);
  }

  /// Check that we can insert an item.
  #[test]
  fn insert_item() {
    let items = ["foo", "bar", "baz", "foobar"];

    let mut db = Db::from_iter(items);
    let item = db.insert(0, "foobarbaz").deref().clone();
    let idx = db.find(&item).unwrap().index();
    assert_eq!(idx, 0);

    let item = db.insert(5, "outoffoos").deref().clone();
    let idx = db.find(&item).unwrap().index();
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
    let idx = db.find(&item).unwrap().index();
    assert_eq!(idx, 0);

    let item = db.get(0).unwrap().deref().clone();
    assert!(db.try_insert(5, item).is_none())
  }

  /// Check that we can insert an item at the end of a `Db`.
  #[test]
  fn push_item() {
    let mut db = Db::from_iter([]);
    let item = db.push("foo").deref().clone();
    let idx = db.find(&item).unwrap().index();
    assert_eq!(idx, 0);

    let item = db.push("bar").deref().clone();
    let idx = db.find(&item).unwrap().index();
    assert_eq!(idx, 1);

    let _removed = db.remove(0);
    let item = db.push("baz").deref().clone();
    let idx = db.find(&item).unwrap().index();
    assert_eq!(idx, 1);
  }

  /// Check that we can insert an item at the end of a `Db`, but fail if
  /// it is a duplicate.
  #[test]
  fn try_push_item() {
    let mut db = Db::from_iter(["foo", "boo", "blah"]);
    let item = db.try_push(Rc::new("foo")).unwrap().deref().clone();
    let idx = db.find(&item).unwrap().index();
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
