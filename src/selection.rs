// selection.rs

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


/// A helper object describing the states a selection can be in.
#[derive(Debug, PartialEq)]
enum Selection<T>
where
  T: Copy + PartialEq,
{
  /// When a selection is started from a widget other than the `TabBar`
  /// itself, we only have the widget's ID as the potential start state.
  /// We cannot know its index internal to the `TabBar`.
  Start(T),
  /// Once the `TabBar` has had a word in the selection process, it will
  /// convert the widget ID into an index and we will work with that.
  /// This state contains the index of the widget where the selection
  /// started.
  Normalized(usize),
}

impl<T> Selection<T>
where
  T: Copy + PartialEq,
{
  /// Calculate the selection index the object represents.
  fn index<I>(&self, mut iter: I) -> usize
  where
    I: ExactSizeIterator<Item=T>,
  {
    match *self {
      Selection::Start(start) => iter.position(|x| x == start).unwrap(),
      Selection::Normalized(idx) => idx,
    }
  }
}


/// A struct representing the state necessary to implement selection advancement in a `TabBar`.
// Note that the type parameter is useful only for testing. The program
// effectively only works with `T` being `Id`.
#[derive(Debug, PartialEq)]
pub struct SelectionState<T>
where
  T: Copy + PartialEq,
{
  selection: Selection<T>,
  advanced: usize,
  total: usize,
}

impl<T> SelectionState<T>
where
  T: Copy + PartialEq,
{
  /// Create a new `SelectionState`.
  pub fn new(current: T) -> Self {
    Self {
      selection: Selection::Start(current),
      advanced: 0,
      total: 0,
    }
  }

  /// Advance the selection by one.
  pub fn advance(&mut self) {
    self.advanced += 1;
    self.total += 1;
  }

  /// Check if the selection got advanced.
  pub fn has_advanced(&self) -> bool {
    self.advanced > 0
  }

  /// Reset the cycle state of the selection.
  pub fn reset_cycled(&mut self) {
    self.total = 0
  }

  /// Check whether the selection has cycled through all widgets once.
  pub fn has_cycled(&self, count: usize) -> bool {
    self.total >= count
  }

  /// Normalize the selection based on the given iterator.
  ///
  /// Return the current index.
  pub fn normalize<I>(&mut self, iter: I) -> usize
  where
    I: ExactSizeIterator<Item=T>,
  {
    let count = iter.len();
    let start_idx = self.selection.index(iter);
    let idx = modulo(start_idx + self.advanced, count);

    self.selection = Selection::Normalized(idx);
    self.advanced = 0;
    idx
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  type TestSelectionState = SelectionState<u16>;


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
  fn selection_state_immediate_advancement() {
    let mut state = TestSelectionState::new(42);
    let iter = [42, 43, 44].iter().cloned();

    state.advance();
    state.advance();
    state.advance();

    let current = state.normalize(iter.clone());
    assert_eq!(current, 0);
    assert!(state.has_cycled(iter.len()));
  }

  #[test]
  fn selection_state_stays_cycled() {
    let mut state = TestSelectionState::new(7);
    let iter = [8, 7, 6].iter().cloned();

    state.advance();
    state.advance();
    state.advance();

    for _ in 1..200 {
      let _ = state.normalize(iter.clone());
      assert!(state.has_cycled(iter.len()));
    }
  }

  #[test]
  fn selection_state_reset_cycled() {
    let mut state = TestSelectionState::new(4);
    let iter = [3, 9, 4].iter().cloned();

    state.advance();
    state.advance();
    state.advance();
    assert_eq!(state.normalize(iter.clone()), 2);
    assert!(state.has_cycled(iter.len()));

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 0);
    state.reset_cycled();

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 1);
    assert!(!state.has_cycled(iter.len()));

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 2);
    assert!(!state.has_cycled(iter.len()));

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 0);
    assert!(state.has_cycled(iter.len()));

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 1);
    assert!(state.has_cycled(iter.len()));
  }
}
