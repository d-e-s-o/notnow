// Copyright (C) 2022 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::Deserialize;
use serde::Serialize;
use serde_json::from_slice as from_json;
use serde_json::to_vec_pretty as to_json;
use serde_json::Error as JsonError;

use super::Backend;


/// A backend for serializing to and deserializing from JSON.
#[derive(Clone, Copy, Debug)]
pub struct Json;

impl<T> Backend<T> for Json
where
  T: Serialize,
  for<'de> T: Deserialize<'de>,
{
  type Error = JsonError;

  #[inline]
  fn serialize(object: &T) -> Result<Vec<u8>, Self::Error> {
    to_json(object)
  }

  #[inline]
  fn deserialize(data: &[u8]) -> Result<T, Self::Error> {
    from_json(data)
  }
}
