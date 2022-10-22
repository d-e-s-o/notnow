// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::ops::Index;
use std::ops::IndexMut;
use std::slice;


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

impl<T> PartialEq<Self> for Id<T> {
  fn eq(&self, other: &Self) -> bool {
    self.id.eq(&other.id)
  }
}


pub trait Idable<T> {
  fn id(&self) -> Id<T>;
}


/// An iterator over the items in a `Db`.
pub type Iter<'t, T> = slice::Iter<'t, T>;


/// A database for storing arbitrary but fixed data items.
#[derive(Debug)]
pub struct Db<T> {
  /// The data, in a well-defined order.
  data: Vec<T>,
}

impl<T> Db<T> {
  /// Create a database from the items contained in the provided
  /// iterator.
  pub fn try_from_iter<I>(iter: I) -> Result<Self, usize>
  where
    T: Idable<T>,
    I: IntoIterator<Item = T>,
  {
    let slf = Self {
      data: iter.into_iter().collect(),
    };
    Ok(slf)
  }

  /// Look up an item's index given the item's ID.
  #[inline]
  pub fn find(&self, id: Id<T>) -> Option<usize>
  where
    T: Idable<T>,
  {
    self.data.iter().position(|task| task.id() == id)
  }

  /// Insert an item into the database at the given `index`.
  #[inline]
  pub fn insert(&mut self, index: usize, item: T) {
    self.data.insert(index, item)
  }

  /// Insert an item at the end of the database.
  #[inline]
  pub fn push(&mut self, item: T) {
    self.data.push(item)
  }

  /// Remove the item at the provided index.
  #[inline]
  pub fn remove(&mut self, index: usize) -> T {
    self.data.remove(index)
  }

  /// Retrieve an iterator over the items of the database.
  #[inline]
  pub fn iter(&self) -> Iter<'_, T> {
    self.data.iter()
  }
}

impl<T> Index<usize> for Db<T> {
  type Output = T;

  #[inline]
  fn index(&self, index: usize) -> &Self::Output {
    &self.data[index]
  }
}

impl<T> IndexMut<usize> for Db<T> {
  #[inline]
  fn index_mut(&mut self, index: usize) -> &mut Self::Output {
    &mut self.data[index]
  }
}
