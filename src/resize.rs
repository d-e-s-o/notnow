// Copyright (C) 2018,2021 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! Infrastructure for intercepting and handling SIGWINCH signals.
//!
//! The purpose of this module is to install a signal handler for
//! SIGWINCH, intercept them, and send an event indicating that such a
//! signal was received through a supplied `std::sync::mpsc::Sender`
//! part of a `std::sync::mpsc::channel`.
//!
//! Signal handling is icky and a signal handler is a particularly
//! disgusting context to run in. It is inherently unsafe because there
//! are severe limitations as to what functions may be invoked. More
//! specifically, we must not invoke anything that grabs a lock in order
//! to guarantee dead lock freedom (a signal handler may interrupt and
//! stop the running thread at an arbitrary time, including when it has
//! acquired a lock; given that the thread will not be running we would
//! deadlock if we were to attempt to acquire the same lock).
//!
//! We would ideally want to directly send an event through a channel
//! from a signal handler, but it is left undefined whether the channel
//! primitive may use locks. To be safe, we instead have a thread
//! waiting on a pipe and just send a byte through this pipe to cause a
//! wake up. The thread will subsequently write to the channel as
//! desired -- from a context that is safe.
//!
//! Note that with signalfd(2) another primitive exist that takes care
//! of part of the work. However, it was not chosen for two reasons:
//! 1) It is Linux specific.
//! 2) It requires us to mask the signal we want to receive with it on
//!    all threads. That means we need to somehow run disabling logic in
//!    all threads we would want to create, ever.

use std::cmp::PartialOrd;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::sync::mpsc::Sender;
use std::thread;

use libc::c_int;
use libc::c_void;
use libc::pipe;
use libc::read;
use libc::SIG_ERR;
use libc::signal;
use libc::SIGWINCH;
use libc::size_t;
use libc::write;

use crate::Event;


/// The file descriptor for a write end of a pipe used for signaling resize events.
static mut WRITE_FD: c_int = 0;


/// Check the return value of a system call.
fn check<T>(result: T, error: T) -> Result<()>
where
  T: Copy + PartialOrd<T>,
{
  if result == error {
    Err(Error::last_os_error())
  } else {
    Ok(())
  }
}

/// A signal handler for SIGWINCH that sends a byte through `WRITE_FD` to wake up a thread.
extern "C" fn handler(signum: c_int) {
  debug_assert_eq!(signum, SIGWINCH);

  let buffer = [0];
  let result = unsafe { write(WRITE_FD, buffer.as_ptr() as *const c_void, 1) };
  check(result, -1).unwrap();
}

/// Register a signal handler for `SIGWINCH` and send a `Event::Resize` object upon interception.
pub fn receive_window_resizes(send_event: Sender<Result<Event>>) -> Result<()> {
  let mut fds = [0, 0];
  unsafe {
    check(pipe(fds.as_mut_ptr()), -1)?;
    check(signal(SIGWINCH, handler as size_t), SIG_ERR)?;
    WRITE_FD = fds[1]
  }
  let read_fd = fds[0];

  let _ = thread::spawn(move || {
    loop {
      let mut buffer = [0];
      let result = unsafe { read(read_fd, buffer.as_mut_ptr() as *mut c_void, 1) };
      if result < 0 {
        if Error::last_os_error().kind() != ErrorKind::Interrupted {
          continue
        }
      }

      let result = check(result, -1).map(|_| Event::Resize);
      send_event.send(result).unwrap();
    }
  });
  Ok(())
}
