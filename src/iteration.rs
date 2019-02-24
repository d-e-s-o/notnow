// iteration.rs

// *************************************************************************
// * Copyright (C) 2018-2019 Daniel Mueller (deso@posteo.net)              *
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

use std::ops::Add;
use std::ops::Rem;


/// Calculate `x` modulo `y`.
fn modulo<T>(x: T, y: T) -> <<<T as Rem>::Output as Add<T>>::Output as Rem<T>>::Output
where
  T: Copy + Rem<T>,
  <T as Rem>::Output: Add<T>,
  <<T as Rem>::Output as Add<T>>::Output: Rem<T>,
{
  ((x % y) + y) % y
}


/// A helper object describing the states an iteration can be in.
#[derive(Debug, PartialEq)]
enum Iteration<T>
where
  T: Copy + PartialEq,
{
  /// When a iteration is started from a widget other than the `TabBar`
  /// itself, we only have the widget's ID as the potential start state.
  /// We cannot know its index internal to the `TabBar`.
  Start(T),
  /// Once the `TabBar` has had a word in the iteration process, it will
  /// convert the widget ID into an index and we will work with that.
  /// This state contains the index of the widget where the iteration
  /// started.
  Normalized(usize),
}

impl<T> Iteration<T>
where
  T: Copy + PartialEq,
{
  /// Calculate the iteration index the object represents.
  fn index<I>(&self, mut iter: I) -> usize
  where
    I: ExactSizeIterator<Item=T>,
  {
    match *self {
      Iteration::Start(start) => iter.position(|x| x == start).unwrap(),
      Iteration::Normalized(idx) => idx,
    }
  }
}


/// A struct representing the state necessary to implement iteration in a `TabBar`.
// Note that the type parameter is useful only for testing. The program
// effectively only works with `T` being `Id`.
#[derive(Debug, PartialEq)]
pub struct IterationState<T>
where
  T: Copy + PartialEq,
{
  iteration: Iteration<T>,
  reversed: bool,
  advanced: isize,
  total: isize,
}

impl<T> IterationState<T>
where
  T: Copy + PartialEq,
{
  /// Create a new `IterationState`.
  pub fn new(current: T) -> Self {
    Self {
      iteration: Iteration::Start(current),
      reversed: false,
      advanced: 0,
      total: 0,
    }
  }

  /// Reverse the iteration, i.e., select in a counter clock-wise fashion.
  pub fn reverse(&mut self, reverse: bool) {
    self.reversed = reverse
  }

  /// Check if the iteration is happening in counter clock-wise fashion or not.
  pub fn is_reversed(&self) -> bool {
    self.reversed
  }

  /// Advance the iteration by one.
  pub fn advance(&mut self) {
    let change = if self.reversed { -1 } else { 1 };
    self.advanced += change;
    self.total += change;
  }

  /// Check if the iteration got advanced.
  pub fn has_advanced(&self) -> bool {
    self.advanced != 0
  }

  /// Reset the cycle state of the iteration.
  pub fn reset_cycled(&mut self) {
    self.total = 0
  }

  /// Check whether the iteration has cycled through all widgets once.
  pub fn has_cycled(&self, count: usize) -> bool {
    // Note that we allow for a single overlap here. That is required
    // because a search (which uses a iteration) may start in the middle
    // of a widget and so we need to be sure to revisit the widget again
    // after we cycled to identify items before the one where the search
    // started. Ultimately we are not so much interested in having an
    // accurate cycle detection, we just need any.
    self.total.abs() as usize > count
  }

  /// Normalize the iteration based on the given iterator.
  ///
  /// Return the current index.
  pub fn normalize<I>(&mut self, iter: I) -> usize
  where
    I: ExactSizeIterator<Item=T>,
  {
    let count = iter.len() as isize;
    let start_idx = self.iteration.index(iter) as isize;
    let idx = modulo(start_idx + self.advanced, count) as usize;

    self.iteration = Iteration::Normalized(idx);
    self.advanced = 0;
    idx
  }
}


#[allow(clippy::cyclomatic_complexity)]
#[cfg(test)]
mod tests {
  use super::*;

  type TestIterationState = IterationState<u16>;


  #[test]
  fn modulo_results() {
    assert_eq!(modulo(-4, 3), 2);
    assert_eq!(modulo(-3, 3), 0);
    assert_eq!(modulo(-2, 3), 1);
    assert_eq!(modulo(-1, 3), 2);
    assert_eq!(modulo(0, 3), 0);
    assert_eq!(modulo(1, 3), 1);
    assert_eq!(modulo(2, 3), 2);
    assert_eq!(modulo(3, 3), 0);
    assert_eq!(modulo(4, 3), 1);
    assert_eq!(modulo(5, 3), 2);
  }

  #[test]
  fn iteration_state_immediate_advancement() {
    let mut state = TestIterationState::new(42);
    let iter = [42, 43, 44].iter().cloned();

    state.advance();
    state.advance();
    state.advance();
    state.advance();

    let current = state.normalize(iter.clone());
    assert_eq!(current, 1);
    assert!(state.has_cycled(iter.len()));
  }

  #[test]
  fn iteration_state_stays_cycled() {
    let mut state = TestIterationState::new(7);
    let iter = [8, 7, 6].iter().cloned();

    state.advance();
    state.advance();
    state.advance();
    state.advance();

    for _ in 1..200 {
      let _ = state.normalize(iter.clone());
      assert!(state.has_cycled(iter.len()));
    }
  }

  #[test]
  fn iteration_state_reset_cycled() {
    let mut state = TestIterationState::new(4);
    let iter = [3, 9, 4].iter().cloned();

    state.advance();
    state.advance();
    state.advance();
    state.advance();
    assert_eq!(state.normalize(iter.clone()), 0);
    assert!(state.has_cycled(iter.len()));

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 1);
    state.reset_cycled();

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 2);
    assert!(!state.has_cycled(iter.len()));

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 0);
    assert!(!state.has_cycled(iter.len()));

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 1);
    assert!(!state.has_cycled(iter.len()));

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 2);
    assert!(state.has_cycled(iter.len()));
  }

  #[test]
  fn reverse_iteration() {
    let mut state = TestIterationState::new(1);
    let iter = [2, 1, 3].iter().cloned();

    state.reverse(true);
    assert!(!state.has_cycled(iter.len()));
    assert!(!state.has_advanced());

    state.advance();
    assert!(state.has_advanced());
    assert_eq!(state.normalize(iter.clone()), 0);
    assert!(!state.has_cycled(iter.len()));
    assert!(!state.has_advanced());

    state.advance();
    assert!(state.has_advanced());
    assert_eq!(state.normalize(iter.clone()), 2);
    assert!(!state.has_cycled(iter.len()));
    assert!(!state.has_advanced());

    state.advance();
    assert!(state.has_advanced());
    assert_eq!(state.normalize(iter.clone()), 1);
    assert!(!state.has_cycled(iter.len()));
    assert!(!state.has_advanced());

    state.reverse(false);
    assert!(!state.has_advanced());
    assert_eq!(state.normalize(iter.clone()), 1);
    assert!(!state.has_cycled(iter.len()));
    assert!(!state.has_advanced());

    state.advance();
    assert!(state.has_advanced());
    assert_eq!(state.normalize(iter.clone()), 2);
    assert!(!state.has_cycled(iter.len()));
    assert!(!state.has_advanced());

    state.advance();
    assert!(state.has_advanced());
    assert_eq!(state.normalize(iter.clone()), 0);
    assert!(!state.has_cycled(iter.len()));
    assert!(!state.has_advanced());
  }
}
