// Copyright (C) 2018-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module housing serialization related definitions.

#![allow(missing_docs)]

pub mod id;
pub mod state;
pub mod tags;
pub mod tasks;
pub mod view;


/// A trait for types that can be converted into a serializable representation.
pub trait ToSerde<T> {
  /// Create a serializable representation of `Self`.
  fn to_serde(&self) -> T;
}
