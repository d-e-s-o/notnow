// Copyright (C) 2018-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module for the serialization of IDs.

use std::convert::TryFrom as _;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::str::FromStr;

use anyhow::Context as _;
use anyhow::Error;

use serde::de::Deserialize;
use serde::de::Deserializer;
use serde::de::Error as _;
use serde::de::Unexpected;
use serde::ser::Serialize;
use serde::ser::Serializer;


/// An ID that can be serialized and deserialized.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Id<T> {
  id: NonZeroUsize,
  phantom: PhantomData<T>,
}

impl<T> Id<T> {
  /// Create a new `Id` object from a "raw" integer.
  pub fn new(id: NonZeroUsize) -> Self {
    Self {
      id,
      phantom: PhantomData,
    }
  }

  /// Retrieve the underlying integer value.
  pub fn get(&self) -> usize {
    self.id.get()
  }
}

impl<T> TryFrom<usize> for Id<T> {
  type Error = Error;

  fn try_from(other: usize) -> Result<Self, Self::Error> {
    let id = NonZeroUsize::new(other).context("encountered an ID of reserved value 0")?;
    Ok(Id::new(id))
  }
}

impl<T> Debug for Id<T> {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    write!(f, "Id {{ id: {} }}", self.id)
  }
}

impl<T> Display for Id<T> {
  /// Format the `Id` into the given formatter.
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    write!(f, "{}", self.id)
  }
}

impl<T> FromStr for Id<T> {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let id = usize::from_str(s)?;
    let id = Id::try_from(id)?;
    Ok(id)
  }
}

// We manually implement Serialize and Deserialize in order to have the
// ID represented as a literal value, and not some structured type.
impl<T> Serialize for Id<T> {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_u64(self.id.get() as u64)
  }
}

impl<'de, T> Deserialize<'de> for Id<T> {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let id = u64::deserialize(deserializer)?;
    let id = NonZeroUsize::try_from(id as usize).map_err(|_| {
      D::Error::invalid_value(Unexpected::Unsigned(id), &"a non-zero unsigned integer")
    })?;

    Ok(Self {
      id,
      phantom: PhantomData,
    })
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use serde_json::from_str as from_json;
  use serde_json::to_string as to_json;

  type TestId = Id<u32>;


  #[test]
  fn serialize_deserialize_id() {
    let id = TestId::try_from(42).unwrap();
    let serialized = to_json(&id).unwrap();
    let deserialized = from_json::<TestId>(&serialized).unwrap();

    assert_eq!(deserialized, id);
  }

  #[test]
  fn serialize_as_number() {
    let id = TestId::try_from(1337).unwrap();
    let serialized = to_json(&id).unwrap();

    assert_eq!(serialized, "1337");
  }
}
