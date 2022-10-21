// Copyright (C) 2017-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

// We basically deny most lints that "warn" by default, except for
// those that may change in incompatible ways the future. We want to
// avoid build breakages when upgrading to new Rust versions.
#![warn(
  bad_style,
  broken_intra_doc_links,
  dead_code,
  future_incompatible,
  illegal_floating_point_literal_pattern,
  improper_ctypes,
  late_bound_lifetime_arguments,
  missing_copy_implementations,
  missing_debug_implementations,
  missing_docs,
  no_mangle_generic_items,
  non_shorthand_field_patterns,
  nonstandard_style,
  overflowing_literals,
  path_statements,
  patterns_in_fns_without_body,
  private_in_public,
  proc_macro_derive_resolution_fallback,
  renamed_and_removed_lints,
  rust_2018_compatibility,
  rust_2018_idioms,
  stable_features,
  trivial_bounds,
  trivial_numeric_casts,
  type_alias_bounds,
  tyvar_behind_raw_pointer,
  unaligned_references,
  unconditional_recursion,
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
  while_true
)]
#![allow(
  deref_into_dyn_supertrait,
  unreachable_pub,
  clippy::collapsible_if,
  clippy::derive_partial_eq_without_eq,
  clippy::let_and_return,
  clippy::let_unit_value,
  clippy::new_ret_no_self,
  clippy::new_without_default,
  clippy::redundant_field_names
)]

//! A terminal based task management application.

mod colors;
mod db;
mod id;
mod ops;
mod resize;
pub mod ser;
pub mod state;
mod tags;
mod tasks;
#[cfg(any(test, feature = "test"))]
#[allow(unsafe_code)]
pub mod test;
mod ui;
mod view;

use std::env::args_os;
use std::fs::OpenOptions;
use std::io::stdin;
use std::io::stdout;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::thread;

#[cfg(feature = "coredump")]
use cdump::register_panic_handler;

use dirs::config_dir;

use termion::event::Event as TermEvent;
use termion::event::Key;
use termion::input::TermReadEventsAndRaw;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;

use tokio::runtime::Builder;

use gui::Renderer;
use gui::Ui;

use crate::resize::receive_window_resizes;
use crate::state::State;
use crate::ui::event::Event as UiEvent;
use crate::ui::message::Message;
use crate::ui::term_renderer::TermRenderer;
use crate::ui::termui::TermUi;
use crate::ui::termui::TermUiData;


/// An event to be handled by the program.
#[derive(Clone, Debug)]
pub enum Event {
  /// A key that has been received, including the raw input data.
  Key(Key, Vec<u8>),
  /// The window has been resized.
  Resize,
}


/// Retrieve the path to the UI's configuration file.
fn ui_config() -> Result<PathBuf> {
  Ok(
    config_dir()
      .ok_or_else(|| Error::new(ErrorKind::NotFound, "Unable to determine config directory"))?
      .join("notnow")
      .join("notnow.json"),
  )
}

/// Retrieve the path to the program's task configuration file.
fn task_config() -> Result<PathBuf> {
  Ok(
    config_dir()
      .ok_or_else(|| Error::new(ErrorKind::NotFound, "Unable to determine config directory"))?
      .join("notnow")
      .join("tasks.json"),
  )
}

/// Instantiate a key receiver thread and have it send key events through the given channel.
fn receive_keys(send_event: Sender<Result<Event>>) {
  thread::spawn(move || {
    let events = stdin().events_and_raw();
    for event in events {
      let result = match event {
        Ok((TermEvent::Key(key), data)) => Ok(Event::Key(key, data)),
        Ok(..) => continue,
        Err(err) => Err(err),
      };
      send_event.send(result).unwrap();
    }
  });
}

/// Handle events in a loop.
async fn run_loop<R>(
  mut ui: Ui<UiEvent, Message>,
  renderer: &R,
  recv_event: &Receiver<Result<Event>>,
) -> Result<()>
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
        Event::Key(key, _raw) => {
          // Attempt to convert the key. If we fail the reason could be that
          // the key is not supported. We just ignore the failure. The UI
          // could not possibly react to it anyway.
          #[cfg(not(feature = "readline"))]
          let event = UiEvent::Key(key, ());
          #[cfg(feature = "readline")]
          let event = UiEvent::Key(key, _raw);

          if let Some(event) = ui.handle(event).await {
            match event {
              UiEvent::Quit => break 'handler,
              UiEvent::Updated => render = true,
              UiEvent::Key(..) => {},
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
pub fn run_prog<W>(out: W, ui_config: &Path, task_config: &Path) -> Result<()>
where
  W: Write,
{
  let rt = Builder::new_current_thread().build()?;

  let state = State::new(ui_config, task_config)?;
  let screen = AlternateScreen::from(out.into_raw_mode()?);
  let colors = state.0.colors.get().unwrap_or_default();
  let renderer = TermRenderer::new(screen, colors)?;
  let State(ui_state, task_state) = state;
  let path = ui_state.path.clone();

  let (ui, _) = Ui::new(
    || Box::new(TermUiData::new(path, task_state)),
    |id, cap| Box::new(TermUi::new(id, cap, ui_state)),
  );

  let (send_event, recv_event) = channel();
  receive_window_resizes(send_event.clone())?;
  receive_keys(send_event);

  // Initially we need to trigger a render in order to have the most
  // recent data presented.
  ui.render(&renderer);
  rt.block_on(run_loop(ui, &renderer, &recv_event))
}

/// Parse the arguments and run the program.
fn run_with_args() -> Result<()> {
  #[cfg(feature = "coredump")]
  {
    register_panic_handler()
      .map_err(|(ctx, err)| Error::new(ErrorKind::Other, format!("{}: {}", ctx, err)))?;
  }

  let ui_config = ui_config()?;
  let task_config = task_config()?;

  let mut it = args_os();
  match it.len() {
    0 | 1 => run_prog(stdout().lock(), &ui_config, &task_config),
    2 => {
      let path = it.nth(1).unwrap();
      let file = OpenOptions::new().read(false).write(true).open(path)?;
      run_prog(file, &ui_config, &task_config)
    },
    _ => Err(Error::new(
      ErrorKind::InvalidInput,
      "Unsupported number of program arguments",
    )),
  }
}

/// Run the program and handle errors.
pub fn run() -> i32 {
  match run_with_args() {
    Ok(_) => 0,
    Err(err) => {
      eprintln!("Error: {}", err);
      1
    },
  }
}
