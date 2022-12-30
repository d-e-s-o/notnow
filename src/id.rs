// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::hash::Hash;
use std::hash::Hasher;
use std::marker::PhantomData;
use std::num::NonZeroUsize;


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
