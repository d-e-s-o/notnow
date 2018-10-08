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


/// A helper object describing the states a selection can be in.
#[derive(Debug, PartialEq)]
enum Selection<T>
where
  T: Copy + Clone + PartialEq,
{
  /// When a selection is started from a widget other than the `TabBar`
  /// itself, we only have the widget's ID as the potential start state.
  /// We cannot know its index internal to the `TabBar`.
  Start(T),
  /// Once the `TabBar` has had a word in the selection process, it will
  /// convert the widget ID into an index and we will work with that.
  /// This state contains the index of the widget where the selection
  /// started as well as the current index, in that order.
  Normalized(usize, usize),
  /// This state is similar to `Normalized`, except that it also
  /// indicates that we have cycled through all possible selections
  /// (i.e., widgets in the `TabBar`) at least once.
  Cycled(usize, usize),
}

impl<T> Selection<T>
where
  T: Copy + Clone + PartialEq,
{
  /// Advance a selection.
  fn advance(first: usize,
             current: usize,
             count: usize,
             advancements: usize) -> (Selection<T>, usize) {
    // We may have advanced already but we have not yet cycled
    // through the start value. So first calculate how many times we
    // already advanced.
    let advanced = if current >= first {
      current - first
    } else {
      count - first + current
    };

    let cycles = (advanced + advancements) / count;
    let new = (current + advancements) % count;
    if cycles >= 1 {
      (Selection::Cycled(first, new), new)
    } else {
      (Selection::Normalized(first, new), new)
    }
  }

  /// Normalize the internals and return the current index.
  fn normalize<I>(&mut self, mut iter: I, advancements: usize) -> usize
  where
    I: ExactSizeIterator<Item=T> + Clone,
  {
    let count = iter.len();

    let (state, new) = match self {
      Selection::Start(id) => {
        let first = iter.position(|x| x == *id).unwrap();
        Self::advance(first, first, count, advancements)
      },
      Selection::Normalized(first, current) => {
        Self::advance(*first, *current, count, advancements)
      },
      Selection::Cycled(first, current) => {
        let new = (*current + advancements) % count;
        (Selection::Cycled(*first, new), new)
      },
    };

    *self = state;
    new
  }

  /// Check whether the selection has cycled through all widgets once.
  fn has_cycled(&self) -> bool {
    match *self {
      Selection::Start(_) |
      Selection::Normalized(_, _) => false,
      Selection::Cycled(_, _) => true,
    }
  }

  /// Reset the cycle state of the selection.
  fn reset_cycled(&mut self) {
    *self = match *self {
      Selection::Start(id) => Selection::Start(id),
      Selection::Normalized(_, current) |
      Selection::Cycled(_, current) => Selection::Normalized(current, current),
    }
  }
}


/// A struct representing the state necessary to implement selection advancement in a `TabBar`.
// Note that the type parameter is useful only for testing. The program
// effectively only works with `T` being `Id`.
#[derive(Debug, PartialEq)]
pub struct SelectionState<T>
where
  T: Copy + Clone + PartialEq,
{
  selection: Selection<T>,
  advanced: usize,
}

impl<T> SelectionState<T>
where
  T: Copy + Clone + PartialEq,
{
  /// Create a new `SelectionState`.
  pub fn new(current: T) -> Self {
    Self {
      selection: Selection::Start(current),
      advanced: 0,
    }
  }

  /// Advance the selection by one.
  pub fn advance(&mut self) {
    self.advanced += 1
  }

  /// Check if the selection got advanced.
  pub fn has_advanced(&self) -> bool {
    self.advanced > 0
  }

  /// Reset the cycle state of the selection.
  pub fn reset_cycled(&mut self) {
    self.selection.reset_cycled()
  }

  /// Check whether the selection has cycled through all widgets once.
  pub fn has_cycled(&self) -> bool {
    self.selection.has_cycled()
  }

  /// Normalize the selection based on the given iterator.
  ///
  /// Return the current index.
  pub fn normalize<I>(&mut self, iter: I) -> usize
  where
    I: ExactSizeIterator<Item=T> + Clone,
  {
    let current = self.selection.normalize(iter, self.advanced);
    self.advanced = 0;
    current
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  type TestSelectionState = SelectionState<u16>;


  #[test]
  fn selection_normalization() {
    for start in 8..14 {
      for adv in 0..8 {
        let mut state = Selection::Start(start);
        let iter = [8, 9, 10, 11, 12, 13, 14].into_iter().cloned();

        assert_eq!(state.normalize(iter, adv), (start - 8 + adv) % 7);
        assert_eq!(state.has_cycled(), adv >= 7);
      }
    }
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
    assert!(state.has_cycled());
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
      assert!(state.has_cycled());
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
    assert!(state.has_cycled());

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 0);
    state.reset_cycled();

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 1);
    assert!(!state.has_cycled());

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 2);
    assert!(!state.has_cycled());

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 0);
    assert!(state.has_cycled());

    state.advance();
    assert_eq!(state.normalize(iter.clone()), 1);
    assert!(state.has_cycled());
  }
}
