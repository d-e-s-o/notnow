// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::min;

use crate::ser::ToSerde;


/// A type representing the position of a task relative to two others
/// adjacent to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position(f64);

impl Position {
  /// Create a `Position` object from an integer.
  pub fn from_int(position: usize) -> Self {
    Self(position as f64)
  }

  /// Create a new `Position` with the provided value.
  pub fn new(value: f64) -> Self {
    Self(value)
  }

  /// Create a `Position` object lying between the two provided
  /// positions.
  pub fn between(first: Option<Position>, second: Option<Position>) -> Option<Self> {
    let (first, second) = match (first, second) {
      (Some(first), Some(second)) => (first.0, second.0),
      // We add/subtract 2.0 here so that we get a chance to get a full
      // integer result.
      (Some(first), None) => (first.0, first.0 + 2f64),
      (None, Some(second)) => (second.0 - 2f64, second.0),
      (None, None) => return Some(Position(0f64)),
    };

    between(first, second).map(Self)
  }

  /// Retrieve the position's floating point value.
  #[inline]
  pub fn get(&self) -> f64 {
    self.0
  }
}

impl ToSerde for Position {
  type Output = f64;

  /// Convert the position into a serializable one.
  fn to_serde(&self) -> Self::Output {
    self.get()
  }
}


/// Normalize a value such that it has a non-zero integer part.
///
/// # Notes
/// The caller should make sure not to invoke the function with an input
/// of zero.
fn ensure_has_integer_part(value: f64) -> (f64, i32) {
  // Can't take log of 0.
  debug_assert_ne!(value, 0.0);

  let exponent = -min(value.log10().floor() as i32, 0);
  (value * 10f64.powi(exponent), exponent)
}


/// Find a value between `first` and `second`, if at all possible.
///
/// The result is chosen in such a way that it is close to equidistant
/// from both, while favoring the fewest number of post decimal digits.
fn between(first: f64, second: f64) -> Option<f64> {
  fn approximate_between(first: f64, second: f64) -> Option<f64> {
    // If we already know that we have a non-zero integer part, skip
    // "normalization" in an attempt to keep floats as stable as
    // possible. Also skip if `second` (the higher of the values) is
    // zero. We can't normalize zero, but the remainder of the logic
    // can deal with the value.
    let (mut first, mut second, mut exponent) = if second == 0f64 || second > 1f64 {
      (first, second, 0i32)
    } else {
      let (second, exponent) = ensure_has_integer_part(second);
      (first * 10f64.powi(exponent), second, exponent)
    };

    let floor = second.floor();
    // Check to see if flooring the value already make it lie between
    // first and second. That's for example the case for first = 0.9 and
    // second = 1.5. This check ensures that we give preference to whole
    // integer values.
    let value = if first < floor && floor < second {
      floor
    } else {
      // Otherwise try the floored value lying between the two and see
      // if the result is between first and second, and rinse repeat
      // after multiplying by ten until we found a suitable candidate.
      // TODO: Check if there is a way to get the same result without
      //       having to loop.
      'outer: loop {
        let value = ((second + first) / 2f64).floor();
        if first < value && value < second {
          break 'outer value
        }

        let new_first = first * 10f64;
        let new_second = second * 10f64;

        if new_first == first && new_second == second {
          return None
        }

        exponent = exponent.checked_add(1)?;
        first = new_first;
        second = new_second;

        if second.is_infinite() {
          return None
        }
      }
    };

    // Revert the power application we did earlier.
    let value = if exponent != 0 {
      value / 10f64.powi(exponent)
    } else {
      value
    };
    Some(value)
  }

  // Make sure that first and second are ordered.
  let (first, second) = if first == second {
    return None
  } else if first < second {
    (first, second)
  } else {
    (second, first)
  };

  let result = approximate_between(first, second)?;
  // It is conceivable that, while we determined some value lying in
  // between `first` and `second` while we were working with
  // intermediate values raised to several powers of ten, that after
  // we normalize back again the imprecision of floating point values
  // just stops the result from being in between.
  if first < result && result < second {
    Some(result)
  } else {
    None
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;


  /// Make sure that we can normalize values such that they have a
  /// non-zero integer part.
  #[test]
  fn integer_part_ensurance() {
    assert_eq!(ensure_has_integer_part(0.000000001).0, 1.0);
    assert_eq!(ensure_has_integer_part(0.1).0, 1.0);
    assert_eq!(ensure_has_integer_part(1.0).0, 1.0);
    assert_eq!(ensure_has_integer_part(1.2).0, 1.2);
    assert_eq!(ensure_has_integer_part(1.8).0, 1.8);
    assert_eq!(ensure_has_integer_part(5.0).0, 5.0);
    assert_eq!(ensure_has_integer_part(9.9).0, 9.9);
    assert_eq!(ensure_has_integer_part(10.0).0, 10.0);
    assert_eq!(ensure_has_integer_part(15.0).0, 15.0);
    assert_eq!(ensure_has_integer_part(227.0).0, 227.0);
  }

  /// Check that we can find a suitable value lying between two others.
  #[test]
  fn approximation_between() {
    assert_eq!(between(-1.0, 0.0).unwrap(), -0.5);
    assert_eq!(between(0.001, 0.01).unwrap(), 0.005);
    assert_eq!(between(0.0, 10.0).unwrap(), 5.0);
    assert_eq!(between(10.0, 0.0).unwrap(), 5.0);
    assert_eq!(between(0.9999, 1.1).unwrap(), 1.0);
    assert_eq!(between(1.0, 3.0).unwrap(), 2.0);
    assert_eq!(between(1.0, 1.00002).unwrap(), 1.00001);
    assert_eq!(between(1.5, 2.5).unwrap(), 2.0);
    assert_eq!(between(1.1, 2.9).unwrap(), 2.0);
    assert_eq!(between(1.1, 2.8).unwrap(), 2.0);
    assert_eq!(between(1.0, 10.0).unwrap(), 5.0);
    assert_eq!(between(2.0, 10.0).unwrap(), 6.0);
    assert_eq!(between(3.0, 10.0).unwrap(), 6.0);
    assert_eq!(between(200.0, 200.3).unwrap(), 200.1);
    assert_eq!(between(200.0, 201.0).unwrap(), 200.5);
    assert_eq!(between(200.0, 202.0).unwrap(), 201.0);

    assert_eq!(between(0.0, 0.0), None);
    assert_eq!(between(1.0, 1.0), None);
    assert_eq!(between(200.0, 200.0), None);
  }

  /// Check that we can create a `Position` lying between two others.
  #[test]
  fn position_creation() {
    let first = Position::from_int(1);
    let second = Position::from_int(2);
    let between = Position::between(Some(first), Some(second)).unwrap();
    assert_eq!(between.get(), 1.5);

    let between = Position::between(None, Some(second)).unwrap();
    assert_eq!(between.get(), 1.0);

    let between = Position::between(Some(first), None).unwrap();
    assert_eq!(between.get(), 2.0);

    let between = Position::between(None, None).unwrap();
    assert_eq!(between.get(), 0.0);
  }
}
