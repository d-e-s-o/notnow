// main.rs

// *************************************************************************
// * Copyright (C) 2017-2018 Daniel Mueller (deso@posteo.net)              *
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

#![allow(
  unknown_lints,
  let_and_return,
  redundant_field_names,
)]
// We basically deny most lints that "warn" by default, except for
// "deprecated" (which would be enabled by "warnings"). We want to avoid
// build breakages due to deprecated items. For those a warning (the
// default) is enough.
#![deny(
  bad_style,
  dead_code,
  duplicate_associated_type_bindings,
  illegal_floating_point_literal_pattern,
  improper_ctypes,
  intra_doc_link_resolution_failure,
  late_bound_lifetime_arguments,
  missing_debug_implementations,
  missing_docs,
  no_mangle_generic_items,
  non_shorthand_field_patterns,
  nonstandard_style,
  overflowing_literals,
  path_statements,
  patterns_in_fns_without_body,
  plugin_as_library,
  private_in_public,
  private_no_mangle_fns,
  private_no_mangle_statics,
  proc_macro_derive_resolution_fallback,
  renamed_and_removed_lints,
  safe_packed_borrows,
  stable_features,
  trivial_bounds,
  type_alias_bounds,
  tyvar_behind_raw_pointer,
  unconditional_recursion,
  unions_with_drop_fields,
  unnameable_test_functions,
  unreachable_code,
  unreachable_patterns,
  unstable_features,
  unstable_name_collisions,
  unused,
  unused_comparisons,
  unused_import_braces,
  unused_lifetimes,
  unused_qualifications,
  where_clauses_object_safety,
  while_true,
)]

//! A terminal based task management application.

extern crate gui;
#[macro_use]
extern crate gui_derive;
extern crate libc;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate termion;

mod event;
mod id;
mod in_out;
mod query;
mod resize;
mod ser;
mod state;
mod tab_bar;
mod tags;
mod task_list_box;
mod tasks;
mod term_renderer;
mod termui;
#[cfg(test)]
#[allow(unsafe_code)]
mod test;

use std::env;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::io::stdin;
use std::io::stdout;
use std::path::PathBuf;
use std::process::exit;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::thread;

use termion::event::Key;
use termion::input::TermRead;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;

use gui::Event as GuiEvent;
use gui::MetaEvent as GuiMetaEvent;
use gui::Renderer;
use gui::Ui;
use gui::UiEvent;

use event::convert;
use resize::receive_window_resizes;
use state::State;
use term_renderer::TermRenderer;
use termui::TermUi;
use termui::TermUiEvent;


/// A type indicating the desire to continue execution.
///
/// If set to `Some` we continue, in case of `None` we stop. The boolean
/// indicates whether or not an `Updated` event was found, indicating
/// that the UI should be rerendered.
type Continue = Option<bool>;


/// An event to be handled by the program.
#[derive(Debug)]
pub enum Event {
  /// A key that has been received.
  Key(Key),
  /// The window has been resized.
  Resize,
}


/// Retrieve the path to the program's configuration file.
fn prog_config() -> Result<PathBuf> {
  Ok(
    env::home_dir()
      .ok_or_else(|| Error::new(
        ErrorKind::NotFound, "Unable to determine home directory"
      ))?
      .join(".config")
      .join("notnow")
      .join("notnow.json"),
  )
}

/// Retrieve the path to the program's task configuration file.
fn task_config() -> Result<PathBuf> {
  Ok(
    env::home_dir()
      .ok_or_else(|| Error::new(
        ErrorKind::NotFound, "Unable to determine home directory"
      ))?
      .join(".config")
      .join("notnow")
      .join("tasks.json"),
  )
}

/// Handle the given `UiEvent`.
fn handle_ui_event(event: UiEvent) -> Continue {
  match event {
    UiEvent::Quit => None,
    UiEvent::Event(GuiEvent::Custom(data)) => {
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

/// Handle the given `GuiMetaEvent`.
fn handle_meta_event(event: GuiMetaEvent) -> Continue {
  match event {
    GuiMetaEvent::UiEvent(ui_event) => handle_ui_event(ui_event),
    GuiMetaEvent::Chain(ui_event, meta_event) => {
      handle_ui_event(ui_event)?;
      handle_meta_event(*meta_event)
    },
  }
}

/// Instantiate a key receiver thread and have it send key events through the given channel.
fn receive_keys(send_event: Sender<Result<Event>>) {
  thread::spawn(move || {
    let keys = stdin().keys();
    for key in keys {
      let event = key.map(|x| Event::Key(x));
      send_event.send(event).unwrap();
    }
  });
}

/// Handle events in a loop.
fn run_loop<R>(mut ui: Ui, renderer: R, recv_event: Receiver<Result<Event>>) -> Result<()>
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
        Event::Key(key) => {
          // Attempt to convert the key. If we fail the reason could be that
          // the key is not supported. We just ignore the failure. The UI
          // could not possibly react to it anyway.
          if let Ok(key) = convert(key) {
            if let Some(event) = ui.handle(GuiEvent::KeyDown(key)) {
              match handle_meta_event(event) {
                Some(update) => render = update || render,
                None => break 'handler,
              }
            }
          }
        },
        Event::Resize => render = true,
      }
    }

    if render {
      ui.render(&renderer);
    }
  }
  Ok(())
}

/// Run the program.
fn run_prog() -> Result<()> {
  let prog_path = prog_config()?;
  let task_path = task_config()?;
  let mut state = Some(State::new(&prog_path, &task_path)?);
  let screen = AlternateScreen::from(stdout().into_raw_mode()?);
  let renderer = TermRenderer::new(screen)?;
  let (ui, _) = Ui::new(&mut |id, cap| {
    let state = state.take().unwrap();
    // TODO: We should be able to propagate errors properly on the `gui`
    //       side of things.
    Box::new(TermUi::new(id, cap, state).unwrap())
  });

  let (send_event, recv_event) = channel();
  receive_window_resizes(send_event.clone())?;
  receive_keys(send_event);

  // Initially we need to trigger a render in order to have the most
  // recent data presented.
  ui.render(&renderer);

  run_loop(ui, renderer, recv_event)
}

/// Run the program and handle errors.
fn run() -> i32 {
  match run_prog() {
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
