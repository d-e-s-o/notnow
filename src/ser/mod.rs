// Copyright (C) 2018-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module housing serialization related definitions.

pub mod backends;
pub mod id;
pub mod state;
pub mod tags;
pub mod tasks;
pub mod view;


/// A trait for types that can be converted into a serializable representation.
pub trait ToSerde {
  /// The result being produced.
  type Output;

  /// Create a serializable representation of `Self`.
  fn to_serde(&self) -> Self::Output;
}
