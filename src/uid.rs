// Copyright (C) 2018,2021-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

pub use uid::Id as Uid;

use crate::ser::id::Id as SerId;
use crate::ser::ToSerde;


impl<T, U> ToSerde<SerId<U>> for Uid<T>
where
  T: Copy,
  U: Copy,
{
  /// Convert this `Uid` into a serializable one.
  ///
  /// Note that it is generally safe to convert this unique in-memory ID
  /// into a serializable one. However, the inverse conversion is not
  /// allowed, for there is no way to guarantee uniqueness of the
  /// resulting in-memory ID.
  fn to_serde(&self) -> SerId<U> {
    // SANITY: Any `uid::Id` is guaranteed to never be zero, so the
    //         unwrap is fine.
    SerId::try_from(self.get()).unwrap()
  }
}
