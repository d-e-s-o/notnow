// Copyright (C) 2018-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module housing serialization related definitions.

pub mod backends;
pub mod id;
#[allow(missing_docs)]
pub mod state;
#[allow(missing_docs)]
pub mod tags;
#[allow(missing_docs)]
pub mod tasks;
#[allow(missing_docs)]
pub mod view;


/// A trait for types that can be converted into a serializable representation.
pub trait ToSerde {
  /// The result being produced.
  type Output;

  /// Create a serializable representation of `Self`.
  fn to_serde(&self) -> Self::Output;
}
