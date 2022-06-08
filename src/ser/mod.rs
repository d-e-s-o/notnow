// Copyright (C) 2018-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod id;
pub mod state;
pub mod tags;
pub mod tasks;
pub mod view;


/// A trait for types that can be converted into a serializable representation.
pub trait ToSerde<T> {
  fn to_serde(&self) -> T;
}
