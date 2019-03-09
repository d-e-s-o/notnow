// main.rs

// *************************************************************************
// * Copyright (C) 2017-2019 Daniel Mueller (deso@posteo.net)              *
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

// We basically deny most lints that "warn" by default, except for
// those that may change in incompatible ways the future. We want to
// avoid build breakages when upgrading to new Rust versions.
#![deny(
  dead_code,
  illegal_floating_point_literal_pattern,
  improper_ctypes,
  intra_doc_link_resolution_failure,
  late_bound_lifetime_arguments,
  missing_copy_implementations,
  missing_debug_implementations,
  missing_docs,
  no_mangle_generic_items,
  non_shorthand_field_patterns,
  overflowing_literals,
  path_statements,
  patterns_in_fns_without_body,
  plugin_as_library,
  private_in_public,
  proc_macro_derive_resolution_fallback,
  safe_packed_borrows,
  stable_features,
  trivial_bounds,
  trivial_numeric_casts,
  type_alias_bounds,
  tyvar_behind_raw_pointer,
  unconditional_recursion,
  unions_with_drop_fields,
  unreachable_code,
  unreachable_patterns,
  unstable_features,
  unstable_name_collisions,
  unused,
  unused_comparisons,
  unused_import_braces,
  unused_lifetimes,
  unused_qualifications,
  unused_results,
  where_clauses_object_safety,
  while_true,
)]
#![warn(
  bad_style,
  future_incompatible,
  nonstandard_style,
  renamed_and_removed_lints,
  rust_2018_compatibility,
  rust_2018_idioms,
)]
#![allow(
  unreachable_pub,
  clippy::collapsible_if,
  clippy::let_and_return,
  clippy::let_unit_value,
  clippy::new_ret_no_self,
  clippy::redundant_field_names,
)]

//! A terminal based task management application.

mod id;
mod query;
mod resize;
mod ser;
mod state;
mod tags;
mod tasks;
#[cfg(test)]
#[allow(unsafe_code)]
mod test;
mod ui;

use std::alloc::System;
use std::env::args_os;
use std::fs::OpenOptions;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::io::stdin;
use std::io::stdout;
use std::io::Write;
use std::path::PathBuf;
use std::process::exit;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::thread;

use dirs::config_dir;

use termion::event::Event as TermEvent;
use termion::event::Key;
use termion::input::TermReadEventsAndRaw;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;

use gui::ChainEvent;
use gui::Renderer;
use gui::Ui;
use gui::UnhandledEvent;
use gui::UnhandledEvents;

use crate::resize::receive_window_resizes;
use crate::state::State;
use crate::ui::event::Event as UiEvent;
use crate::ui::term_renderer::TermRenderer;
use crate::ui::termui::TermUi;
use crate::ui::termui::TermUiEvent;

// Switch from the default allocator (typically jemalloc) to the system
// allocator (malloc based on Unix systems). Our application is by no
// means allocation intensive and the default allocator is typically
// much larger in size, causing binary bloat.
#[global_allocator]
static A: System = System;

/// A type indicating the desire to continue execution.
///
/// If set to `Some` we continue, in case of `None` we stop. The boolean
/// indicates whether or not an `Updated` event was found, indicating
/// that the UI should be rerendered.
type Continue = Option<bool>;


/// An event to be handled by the program.
#[derive(Clone, Debug)]
pub enum Event {
  /// A key that has been received, including the raw input data.
  Key(Key, Vec<u8>),
  /// The window has been resized.
  Resize,
}


/// Retrieve the path to the program's task configuration file.
fn task_config() -> Result<PathBuf> {
  Ok(
    config_dir()
      .ok_or_else(|| Error::new(
        ErrorKind::NotFound, "Unable to determine config directory"
      ))?
      .join("notnow")
      .join("tasks.json"),
  )
}

/// Retrieve the path to the UI's configuration file.
fn ui_config() -> Result<PathBuf> {
  Ok(
    config_dir()
      .ok_or_else(|| Error::new(
        ErrorKind::NotFound, "Unable to determine config directory"
      ))?
      .join("notnow")
      .join("notnow.json"),
  )
}

/// Handle the given `UnhandledEvent`.
fn handle_unhandled_event(event: UnhandledEvent<UiEvent>) -> Continue {
  match event {
    UnhandledEvent::Quit => None,
    UnhandledEvent::Custom(data) => {
      match data.downcast::<TermUiEvent>() {
        Ok(event) => {
          match *event {
            TermUiEvent::Updated => Some(true),
            _ => panic!("Unexpected TermUiEvent variant escaped: {:?}", event),
          }
        },
        Err(event) => panic!("Received unexpected custom event: {:?}", event),
      }
    },
    _ => Some(false),
  }
}

/// Handle the given chain of `UnhandledEvent` objects.
fn handle_unhandled_events(events: UnhandledEvents<UiEvent>) -> Continue {
  match events {
    ChainEvent::Event(event) => handle_unhandled_event(event),
    ChainEvent::Chain(event, chain) => {
      let _ = handle_unhandled_event(event)?;
      handle_unhandled_events(*chain)
    },
  }
}

/// Instantiate a key receiver thread and have it send key events through the given channel.
fn receive_keys(send_event: Sender<Result<Event>>) {
  let _ = thread::spawn(move || {
    let events = stdin().events_and_raw();
    for event in events {
      let result = match event {
        Ok((TermEvent::Key(key), data)) => Ok(Event::Key(key, data)),
        Ok(..) => continue,
        Err(err) => Err(err)
      };
      send_event.send(result).unwrap();
    }
  });
}

/// Handle events in a loop.
fn run_loop<R>(mut ui: Ui<UiEvent>,
               renderer: &R,
               recv_event: &Receiver<Result<Event>>) -> Result<()>
where
  R: Renderer,
{
  'handler: loop {
    let mut render = false;
    // We want to read keys in batches in order to avoid unnecessary
    // render invocations, for example when a user pastes text (where
    // each key would result in a UI update). To make that happen we
    // queue them up in a background thread and then drain them here. We
    // use a single recv call to block once for an event and then use an
    // iterator to read and handle every key event queued up to this
    // point.
    let event = recv_event.recv().unwrap();
    for event in Some(event).into_iter().chain(recv_event.try_iter()) {
      match event? {
        Event::Key(key, raw) => {
          // Attempt to convert the key. If we fail the reason could be that
          // the key is not supported. We just ignore the failure. The UI
          // could not possibly react to it anyway.
          #[cfg(not(feature = "readline"))]
          let event = { let _ = raw; UiEvent::Key(key, ()) };
          #[cfg(feature = "readline")]
          let event = UiEvent::Key(key, raw);

          if let Some(event) = ui.handle(event) {
            match handle_unhandled_events(event) {
              Some(update) => render = update || render,
              None => break 'handler,
            }
          }
        },
        Event::Resize => render = true,
      }
    }

    if render {
      ui.render(renderer);
    }
  }
  Ok(())
}

/// Run the program.
fn run_prog<W>(out: W) -> Result<()>
where
  W: Write,
{
  let task_path = task_config()?;
  let ui_path = ui_config()?;

  let mut state = Some(State::new(&task_path, &ui_path)?);
  let screen = AlternateScreen::from(out.into_raw_mode()?);
  let renderer = TermRenderer::new(screen)?;
  let (ui, _) = Ui::new(&mut |id, cap| {
    Box::new(TermUi::new(id, cap, state.take().unwrap()))
  });

  let (send_event, recv_event) = channel();
  receive_window_resizes(send_event.clone())?;
  receive_keys(send_event);

  // Initially we need to trigger a render in order to have the most
  // recent data presented.
  ui.render(&renderer);

  run_loop(ui, &renderer, &recv_event)
}

/// Parse the arguments and run the program.
fn run_with_args() -> Result<()> {
  let mut it = args_os();
  match it.len() {
    0 | 1 => run_prog(stdout()),
    2 => {
      let path = it.nth(1).unwrap();
      let file = OpenOptions::new().read(false).write(true).open(path)?;
      run_prog(file)
    },
    _ => Err(Error::new(ErrorKind::InvalidInput, "unsupported number of arguments")),
  }
}

/// Run the program and handle errors.
fn run() -> i32 {
  match run_with_args() {
    Ok(_) => 0,
    Err(err) => {
      eprintln!("Error: {}", err);
      1
    },
  }
}

fn main() {
  exit(run());
}
