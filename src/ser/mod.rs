// Copyright (C) 2018,2021 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod id;
pub mod query;
pub mod state;
pub mod tags;
pub mod tasks;


/// A trait for types that can be converted into a serializable representation.
pub trait ToSerde<T> {
  fn to_serde(&self) -> T;
}
