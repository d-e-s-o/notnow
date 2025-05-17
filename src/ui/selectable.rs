// Copyright (C) 2021-2024 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::max;
use std::cmp::min;


/// Sanitize a selection index.
fn sanitize_selection(selection: isize, count: usize) -> usize {
  if count == 0 {
    0
  } else {
    max(0, min(count as isize - 1, selection)) as usize
  }
}


/// A trait representing a changeable selection.
pub trait Selectable {
  /// Retrieve the selection index.
  fn selection_index(&self) -> isize;

  /// Set the selection index.
  fn set_selection_index(&mut self, selection: isize);

  /// Retrieve the item count.
  fn count(&self) -> usize;

  /// Retrieve the sanitized selection.
  fn selection(&self, add: isize) -> usize {
    let count = self.count();
    let index = self.selection_index();
    let selection = sanitize_selection(index, count);
    debug_assert!(add >= 0 || selection as isize >= add);
    (selection as isize + add) as usize
  }

  /// Set the currently selected item.
  fn select(&mut self, selection: isize) -> bool {
    let count = self.count();
    let index = self.selection_index();
    let old_selection = sanitize_selection(index, count);
    let new_selection = sanitize_selection(selection, count);

    self.set_selection_index(selection);
    new_selection != old_selection
  }

  /// Change the currently selected item in a relative fashion.
  fn change_selection(&mut self, change: isize) -> bool {
    // We always make sure to base the given `change` value off of a
    // sanitized selection. Otherwise the result is not as expected.
    let count = self.count();
    let index = self.selection_index();
    let selection = sanitize_selection(index, count);
    let new_selection = selection as isize + change;
    self.select(new_selection)
  }
}
