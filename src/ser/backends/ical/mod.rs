// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

mod task;
mod tasks_meta;
mod util;

use std::error::Error as StdError;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::ops::Deref;

use anyhow::Error;
use anyhow::Result;

use super::Backend;


/// A wrapper around a boxed error that allows us to use it in
/// conjunction with `anyhow`.
///
/// This type is required because `Box<dyn Error>` is lacking an
/// implementation of `std::error::Error`; for more details check
/// https://github.com/rust-lang/rust/issues/60759
#[derive(Debug)]
pub struct E(Box<dyn StdError + Send + Sync>);

impl Display for E {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    Display::fmt(&self.0, f)
  }
}

impl StdError for E {
  fn source(&self) -> Option<&(dyn StdError + 'static)> {
    Some(self.0.deref())
  }
}


/// An internal trait facilitating string conversion via one of the
/// `icalendar` types.
trait SerICal
where
  Self: Sized,
{
  /// Convert `self` into a string representing an iCal object.
  fn to_ical_string(&self) -> String;

  /// Create a `Self` from a string representing an iCal object.
  fn from_ical_string(data: &str) -> Result<Self, Error>;
}


/// A backend for serializing to and deserializing from iCal.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug)]
pub struct iCal;

impl<T> Backend<T> for iCal
where
  T: SerICal,
{
  type Error = E;

  #[inline]
  fn serialize(object: &T) -> Result<Vec<u8>, Self::Error> {
    Ok(T::to_ical_string(object).into_bytes())
  }

  #[inline]
  fn deserialize(data: &[u8]) -> Result<T, Self::Error> {
    let string = String::from_utf8(data.to_vec())
      .map_err(Box::from)
      .map_err(E)?;
    T::from_ical_string(&string).map_err(Box::from).map_err(E)
  }
}
