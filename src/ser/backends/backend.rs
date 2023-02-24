// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::error::Error;


/// A trait abstracting over a specific serialization & deserialization
/// backend.
pub trait Backend<T> {
  /// The error type emitted as part of serialization and
  /// deserialization.
  type Error: Error + Send + Sync + 'static;

  /// Serialize an object to a byte buffer.
  fn serialize(object: &T) -> Result<Vec<u8>, Self::Error>;

  /// Deserialize an object from a byte buffer.
  fn deserialize(data: &[u8]) -> Result<T, Self::Error>;
}
