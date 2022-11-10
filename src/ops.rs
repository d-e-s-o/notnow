// Copyright (C) 2021-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::marker::PhantomData;

use rbuf::RingBuf;


/// A trait representing a reversible operation.
pub trait Op<D, T> {
  /// Execute the operation.
  fn exec(&mut self, data: &mut D) -> T;

  /// Undo the operation.
  fn undo(&mut self, data: &mut D) -> T;
}


/// A "list" of executed operations.
///
/// It is actually a fixed size ring buffer, but conceptually we allow
/// for pushing of new operations and then moving back and forth again.
#[derive(Debug)]
pub struct Ops<O, D, T> {
  /// A fixed size ring buffer storing operations performed on tasks as
  /// well as their inverse (i.e., allowing us to "undo").
  ops: RingBuf<Option<O>>,
  _phantom: PhantomData<(D, T)>,
}

impl<O, D, T> Ops<O, D, T> {
  pub fn new(max_count: usize) -> Self {
    Self {
      // We add one to the maximum undo step count to account for the
      // one sentinel value that we insert that separates the "top of
      // the stack" from earlier operations that were overwritten.
      ops: RingBuf::new(max_count + 1),
      _phantom: PhantomData,
    }
  }
}

impl<O, D, T> Ops<O, D, T>
where
  O: Op<D, T>,
{
  /// Execute an operation and stash it away for later.
  pub fn exec(&mut self, mut op: O, data: &mut D) -> T {
    let result = op.exec(data);

    self.ops.push_front(Some(op));
    // We just inserted a new element, which means that if we still have
    // some operations in the ring buffer that we undid earlier, now is
    // the time to just drop them (we only keep one linear line of
    // operations, not a tree of sorts). Hence, insert a sentinel value
    // replacing the least recently executed operation.
    *self.ops.back_mut() = None;
    result
  }

  /// Undo the most recent operation, returning the result of the action
  /// if one was performed, or `None`.
  pub fn undo(&mut self, data: &mut D) -> Option<T> {
    if let Some(op) = self.ops.front_mut() {
      let result = op.undo(data);

      let op = self.ops.pop_front();
      // We didn't actually need to remove the operation from the ring
      // buffer, but there is no method for just decrementing the front
      // pointer or similar. As such, just put the element back in at
      // what is now the back. This way, it will still be available
      // should we decide to `redo` it.
      *self.ops.back_mut() = op;
      Some(result)
    } else {
      None
    }
  }

  /// Re-do the next operation, returning the result of the action
  /// if one was performed, or `None`.
  pub fn redo(&mut self, data: &mut D) -> Option<T> {
    if let Some(op) = self.ops.back_mut() {
      let result = op.exec(data);

      // There is no way for us to tell the ring buffer to just advance
      // the "front" pointer. So we actually have to take a peek at the
      // "back" element and then push it in there to become the new
      // front.
      let op = self.ops.back_mut().take();
      self.ops.push_front(op);
      Some(result)
    } else {
      None
    }
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;


  struct AddOp(usize);

  impl Op<usize, ()> for AddOp {
    fn exec(&mut self, data: &mut usize) {
      *data += self.0;
    }

    fn undo(&mut self, data: &mut usize) {
      *data -= self.0;
    }
  }


  struct MulOp(usize);

  impl Op<usize, ()> for MulOp {
    fn exec(&mut self, data: &mut usize) {
      *data *= self.0;
    }

    fn undo(&mut self, data: &mut usize) {
      *data /= self.0;
    }
  }

  impl<D, T> Op<D, T> for &mut dyn Op<D, T> {
    fn exec(&mut self, data: &mut D) -> T {
      (*self).exec(data)
    }

    fn undo(&mut self, data: &mut D) -> T {
      (*self).undo(data)
    }
  }


  /// Check that we can execute, undo, and then redo operations.
  #[test]
  fn exec_undo_redo() {
    let mut data = 0;
    let mut ops = Ops::<&mut dyn Op<usize, ()>, usize, ()>::new(3);

    let mut op1 = AddOp(4);
    let mut op2 = MulOp(3);
    let mut op3 = MulOp(7);

    ops.exec(&mut op1, &mut data);
    assert_eq!(data, 4);

    ops.exec(&mut op2, &mut data);
    assert_eq!(data, 12);

    ops.exec(&mut op3, &mut data);
    assert_eq!(data, 84);

    assert!(ops.undo(&mut data).is_some());
    assert_eq!(data, 12);

    assert!(ops.redo(&mut data).is_some());
    assert_eq!(data, 84);

    assert!(ops.undo(&mut data).is_some());
    assert_eq!(data, 12);

    assert!(ops.undo(&mut data).is_some());
    assert_eq!(data, 4);

    assert!(ops.redo(&mut data).is_some());
    assert_eq!(data, 12);

    assert!(ops.undo(&mut data).is_some());
    assert_eq!(data, 4);

    assert!(ops.undo(&mut data).is_some());
    assert_eq!(data, 0);
  }

  /// Check that we can undo and redp the correct number of operations.
  #[test]
  fn undo_limit() {
    let mut data = 0;
    let mut ops = Ops::<&mut dyn Op<usize, ()>, usize, ()>::new(2);

    let mut op1 = AddOp(2);
    let mut op2 = MulOp(3);
    let mut op3 = MulOp(5);

    ops.exec(&mut op1, &mut data);
    assert_eq!(data, 2);

    ops.exec(&mut op2, &mut data);
    assert_eq!(data, 6);

    // Given that we only have room for two operations, this one should
    // evict `op1` from our memory.
    ops.exec(&mut op3, &mut data);
    assert_eq!(data, 30);

    assert!(ops.undo(&mut data).is_some());
    assert_eq!(data, 6);

    assert!(ops.undo(&mut data).is_some());
    assert_eq!(data, 2);

    // We should only be able to undo two operations.
    for _ in 0..5 {
      assert!(ops.undo(&mut data).is_none());
      assert_eq!(data, 2);
    }

    assert!(ops.redo(&mut data).is_some());
    assert_eq!(data, 6);

    assert!(ops.redo(&mut data).is_some());
    assert_eq!(data, 30);

    // Similarly, we may only ever redo two operations.
    for _ in 0..5 {
      assert!(ops.redo(&mut data).is_none());
      assert_eq!(data, 30);
    }

    assert!(ops.undo(&mut data).is_some());
    assert_eq!(data, 6);

    assert!(ops.undo(&mut data).is_some());
    assert_eq!(data, 2);

    for _ in 0..5 {
      assert!(ops.undo(&mut data).is_none());
      assert_eq!(data, 2);
    }
  }
}
