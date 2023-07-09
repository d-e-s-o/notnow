// Copyright (C) 2022-2023 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::Cell;
use std::cell::RefCell;
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
  #[inline]
  pub fn aux(&self) -> Aux {
    self.data[self.index].1.get()
  }

  /// Set the auxiliary data associated with this `Entry`.
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


/// Update the provided `by_ptr_idx` with index data for `data`.
fn update_ptr_idx<T, U>(by_ptr_idx: &mut Vec<(Rc<T>, ReplayIdx)>, data: &[(Rc<T>, U)]) {
  let iter = data.iter().enumerate().map(|(idx, (rc, _aux))| {
    let rep_idx = ReplayIdx::new(idx, Gen(0));
    (rc.clone(), rep_idx)
  });

  let () = by_ptr_idx.clear();
  let () = by_ptr_idx.extend(iter);
  let () = by_ptr_idx.sort_by_key(|(rc, _idx)| Rc::as_ptr(rc));
}


/// Create an index optimized for pointer based access for the provided
/// data.
#[inline]
fn make_ptr_idx<T, U>(data: &[(Rc<T>, U)]) -> Vec<(Rc<T>, ReplayIdx)> {
  let mut idx = Vec::new();
  let () = update_ptr_idx(&mut idx, data);
  idx
}


/// An enumeration representing insertions & deletions of elements at
/// certain indexes in our `Db`.
#[derive(Debug)]
enum InsDel {
  Insert(u32),
  Delete(u32),
}


/// A generation number that doubles as an index into `ins_del`.
#[derive(Debug)]
#[repr(transparent)]
struct Gen(u16);

impl Gen {
  fn new(gen: usize) -> Self {
    let gen = if cfg!(debug_assertions) {
      gen.try_into().unwrap()
    } else {
      gen as _
    };

    Self(gen)
  }
}


/// An index that can be adjusted based on a log of operations performed
/// ("replayed").
#[derive(Debug)]
struct ReplayIdx {
  idx: u32,
  gen: Gen,
}

impl ReplayIdx {
  fn new(idx: usize, gen: Gen) -> Self {
    Self {
      idx: idx as u32,
      gen,
    }
  }

  /// Replay a set of insert/delete operations on this index.
  fn replay(&self, ins_del: &[InsDel]) -> usize {
    let mut idx = self.idx;
    let gen = usize::from(self.gen.0);

    if gen < ins_del.len() {
      for op in &ins_del[gen..] {
        match op {
          InsDel::Insert(ins_idx) => {
            if *ins_idx <= idx {
              idx += 1
            }
          },
          InsDel::Delete(rem_idx) => {
            if *rem_idx <= idx {
              idx -= 1
            }
          },
        }
      }
    }

    idx as usize
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
  /// An index on top of `data` sorted by the `Rc` pointer value for
  /// efficient lookups.
  by_ptr_idx: RefCell<Vec<(Rc<T>, ReplayIdx)>>,
  /// A list of insertions and deletions since we rebuilt `by_ptr_idx`.
  ins_del: Vec<InsDel>,
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

    let slf = Self {
      by_ptr_idx: RefCell::new(make_ptr_idx(&data)),
      data,
      ins_del: Vec::new(),
    };
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
    let data = iter
      .into_iter()
      .map(|(item, aux)| (Rc::new(item), Cell::new(aux)))
      .collect::<Vec<_>>();

    Self {
      by_ptr_idx: RefCell::new(make_ptr_idx(&data)),
      data,
      ins_del: Vec::new(),
    }
  }

  /// Look up a value in our index.
  #[inline]
  fn find_in_index(by_ptr_idx: &[(Rc<T>, ReplayIdx)], rc: &Rc<T>) -> Result<usize, usize> {
    let ptr = Rc::as_ptr(rc);
    by_ptr_idx.binary_search_by_key(&ptr, |(rc, _idx)| Rc::as_ptr(rc))
  }

  /// Rebuild our index if the `ins_del` log became too big.
  #[inline]
  fn maybe_rebuild_by_ptr_idx(&mut self) -> bool {
    // 2^12 (4096) elements seem to be a reasonable sweet spot between:
    // memory consumption is reasonable and with lower values we loose
    // performance, while with higher values we don't gain much.
    if self.ins_del.len() >= 1 << 12 {
      let by_ptr_idx = self.by_ptr_idx.get_mut();
      // Just rebuild the index from scratch.
      let () = update_ptr_idx(by_ptr_idx, &self.data);
      let () = self.ins_del.clear();
      true
    } else {
      false
    }
  }

  /// Add the `idx`th element in the `Db` to the `by_ptr_idx` index.
  ///
  /// # Notes
  /// This method should be called *after* the element at `idx` has
  /// actually been removed from `data`.
  fn index(&mut self, idx: usize) {
    if self.maybe_rebuild_by_ptr_idx() {
      // As per our contract the element at `idx` has already been
      // inserted and so our newly rebuilt index is up-to-date.
      return
    }

    let by_ptr_idx = self.by_ptr_idx.get_mut();
    let () = self.ins_del.push(InsDel::Insert(idx as u32));
    let gen = Gen::new(self.ins_del.len());
    let rep_idx = ReplayIdx::new(idx, gen);

    let rc = &self.data[idx].0;
    let insert_idx = Self::find_in_index(by_ptr_idx, rc);
    match insert_idx {
      Ok(..) => panic!("attempting to index already existing element @ {idx}"),
      Err(insert_idx) => {
        if insert_idx == by_ptr_idx.len() {
          by_ptr_idx.push((rc.clone(), rep_idx))
        } else {
          by_ptr_idx.insert(insert_idx, (rc.clone(), rep_idx))
        }
      },
    }
  }

  /// Remove the `idx`th element in the `Db` from the `by_ptr_idx` index.
  ///
  /// # Notes
  /// This method should be called *before* the element at `idx` is
  /// actually removed from `data`.
  fn deindex(&mut self, idx: usize) {
    let _rebuilt = self.maybe_rebuild_by_ptr_idx();

    // Even if we rebuilt the index from scratch we still have to update
    // it to reflect that fact that the element at `idx` is about to be
    // removed.
    let by_ptr_idx = self.by_ptr_idx.get_mut();

    let () = self.ins_del.push(InsDel::Delete(idx as u32));

    let rc = &self.data[idx].0;
    let remove_idx = Self::find_in_index(by_ptr_idx, rc);
    match remove_idx {
      Ok(remove_idx) => {
        let _idx = by_ptr_idx.remove(remove_idx);
      },
      Err(..) => panic!("attempting to de-index non-existent element @ {idx}"),
    }
  }

  /// Look up an item's `Entry` in the `Db`.
  #[inline]
  pub fn find(&self, item: &Rc<T>) -> Option<Entry<'_, T, Aux>> {
    let mut by_ptr_idx = self.by_ptr_idx.borrow_mut();

    Self::find_in_index(&by_ptr_idx, item).ok().and_then(|idx| {
      let rep_idx = &by_ptr_idx[idx].1;
      let data_idx = rep_idx.replay(&self.ins_del);

      let gen = Gen::new(self.ins_del.len());
      by_ptr_idx[idx].1 = ReplayIdx::new(data_idx, gen);

      self.get(data_idx)
    })
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
    let () = self.index(index);
    // SANITY: We know we just inserted an item at `index`, so an entry
    //         has to exist.
    self.get(index).unwrap()
  }

  /// Try inserting an item into the database at the given `index`.
  ///
  /// This function succeeds if `item` is not yet present.
  #[cfg(test)]
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
      let () = self.index(index);
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
    let rc = Rc::new(item);
    let idx = self.data.len();
    let () = self.data.push((rc, Cell::new(aux)));
    let () = self.index(idx);
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
      let idx = self.data.len();
      let () = self.data.push((item, Cell::new(aux)));
      let () = self.index(idx);
      self.last()
    }
  }

  /// Remove the item at the provided index.
  #[inline]
  pub fn remove(&mut self, index: usize) -> (Rc<T>, Aux) {
    let () = self.deindex(index);
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
    let Self {
      data,
      by_ptr_idx: _,
      ins_del: _,
    } = self;

    f.debug_struct("Db").field("data", &data).finish()
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;

  #[cfg(feature = "nightly")]
  use std::hint::black_box;

  #[cfg(feature = "nightly")]
  use unstable_test::Bencher;


  #[cfg(feature = "nightly")]
  const ITEM_CNT: usize = 10000;


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

  /// Benchmark data insertion at the start of a [`Db`].
  #[cfg(feature = "nightly")]
  #[bench]
  fn bench_insert(b: &mut Bencher) {
    let () = b.iter(|| {
      let mut db = Db::from_iter([]);
      // Reserve all necessary memory in a single allocation so that
      // allocation to minimize allocation overhead.
      let () = db.data.reserve(ITEM_CNT);

      for i in 1..ITEM_CNT {
        let _entry = db.insert(0, black_box(i));
      }
    });
  }

  /// Benchmark data insertion at the start of a [`Db`].
  #[cfg(feature = "nightly")]
  #[bench]
  fn bench_try_insert(b: &mut Bencher) {
    let items = (0..ITEM_CNT).map(Rc::new).collect::<Vec<_>>();

    let () = b.iter(|| {
      let mut db = Db::from_iter([]);
      let () = db.data.reserve(ITEM_CNT);

      for item in items.iter() {
        assert!(db
          .try_insert(black_box(0), black_box(item.clone()))
          .is_some());
      }
    });
  }

  /// Benchmark data insertion at the end of a [`Db`].
  #[cfg(feature = "nightly")]
  #[bench]
  fn bench_push(b: &mut Bencher) {
    let () = b.iter(|| {
      let mut db = Db::from_iter([]);
      let () = db.data.reserve(ITEM_CNT);

      for i in 1..ITEM_CNT {
        let _entry = db.push(black_box(i));
      }
    });
  }

  /// Benchmark checked data insertion at the end of a [`Db`].
  #[cfg(feature = "nightly")]
  #[bench]
  fn bench_try_push(b: &mut Bencher) {
    let items = (0..ITEM_CNT).map(Rc::new).collect::<Vec<_>>();

    let () = b.iter(|| {
      let mut db = Db::from_iter([]);
      let () = db.data.reserve(ITEM_CNT);

      for item in items.iter() {
        assert!(db.try_push(black_box(item.clone())).is_some());
      }
    });
  }

  /// Benchmark data lookup in a [`Db`].
  #[cfg(feature = "nightly")]
  #[bench]
  fn bench_finding(b: &mut Bencher) {
    let mut db = Db::from_iter([]);

    let items = (1..ITEM_CNT)
      .map(|i| db.push(i).deref().clone())
      .collect::<HashSet<_>>();

    let () = b.iter(|| {
      // Lookup each item.
      let () = items.iter().for_each(|item| {
        let _item = db.find(black_box(item)).unwrap();
      });
    });
  }

  /// Benchmark repeated removal of the first item from a [`Db`].
  #[cfg(feature = "nightly")]
  #[bench]
  fn bench_remove_first(b: &mut Bencher) {
    let () = b.iter(|| {
      let mut db = Db::from_iter(0..ITEM_CNT);
      for _ in 0..ITEM_CNT {
        let _item = db.remove(black_box(0));
      }
    });
  }

  /// Benchmark repeated removal of the last item from a [`Db`].
  #[cfg(feature = "nightly")]
  #[bench]
  fn bench_remove_last(b: &mut Bencher) {
    let () = b.iter(|| {
      let mut db = Db::from_iter(0..ITEM_CNT);
      for _ in 0..ITEM_CNT {
        let _item = db.remove(black_box(db.len() - 1));
      }
    });
  }
}
