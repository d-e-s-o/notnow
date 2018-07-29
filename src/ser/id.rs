// id.rs

// *************************************************************************
// * Copyright (C) 2018 Daniel Mueller (deso@posteo.net)                   *
// *                                                                       *
// * This program is free software: you can redistribute it and/or modify  *
// * it under the terms of the GNU General Public License as published by  *
// * the Free Software Foundation, either version 3 of the License, or     *
// * (at your option) any later version.                                   *
// *                                                                       *
// * This program is distributed in the hope that it will be useful,       *
// * but WITHOUT ANY WARRANTY; without even the implied warranty of        *
// * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the         *
// * GNU General Public License for more details.                          *
// *                                                                       *
// * You should have received a copy of the GNU General Public License     *
// * along with this program.  If not, see <http://www.gnu.org/licenses/>. *
// *************************************************************************

use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::marker::PhantomData;

use serde::de::Deserialize;
use serde::de::Deserializer;
use serde::ser::Serialize;
use serde::ser::Serializer;


/// An ID that can be serialized and deserialized.
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct Id<T>
where
  T: Copy,
{
  id: usize,
  phantom: PhantomData<T>,
}

impl<T> Id<T>
where
  T: Copy,
{
  pub fn new(id: usize) -> Self {
    Id {
      id: id,
      phantom: PhantomData,
    }
  }
}

impl<T> Debug for Id<T>
where
  T: Copy,
{
  fn fmt(&self, f: &mut Formatter) -> FmtResult {
    write!(f, "Id {{ id: {} }}", self.id)
  }
}

impl<T> Display for Id<T>
where
  T: Copy,
{
  /// Format the `Id` into the given formatter.
  fn fmt(&self, f: &mut Formatter) -> FmtResult {
    write!(f, "{}", self.id)
  }
}

// We manually implement Serialize and Deserialize in order to have the
// ID represented as a literal value, and not some structured type.
impl<T> Serialize for Id<T>
where
  T: Copy,
{
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_u64(self.id as u64)
  }
}

impl<'de, T> Deserialize<'de> for Id<T>
where
  T: Copy,
{
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    Ok(Id {
      id: u64::deserialize(deserializer)? as usize,
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
    let id = TestId::new(42);
    let serialized = to_json(&id).unwrap();
    let deserialized = from_json::<TestId>(&serialized).unwrap();

    assert_eq!(deserialized, id);
  }

  #[test]
  fn serialize_as_number() {
    let id = TestId::new(1337);
    let serialized = to_json(&id).unwrap();

    assert_eq!(serialized, "1337");
  }
}
