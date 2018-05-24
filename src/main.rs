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
  unsafe_code,
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
extern crate serde_derive;
extern crate serde_json;
extern crate termion;

mod controller;
mod event;
mod tasks;
mod term_renderer;
mod termui;

use std::env;
use std::io::Error;
use std::io::ErrorKind;
use std::io::Result;
use std::io::stdin;
use std::io::stdout;
use std::path::PathBuf;
use std::process::exit;

use termion::input::TermRead;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;

use gui::Ui;
use gui::UiEvent;

use controller::Controller;
use event::convert;
use term_renderer::TermRenderer;
use termui::TermUi;


/// Retrieve the path to the program's configuration file.
fn config() -> Result<PathBuf> {
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

/// Run the program.
fn run_prog() -> Result<()> {
  let task_path = config()?;
  let mut controller = Some(Controller::new(&task_path)?);
  let screen = AlternateScreen::from(stdout().into_raw_mode()?);
  let renderer = TermRenderer::new(screen)?;
  let mut events = stdin().events();
  let (mut ui, _) = Ui::new(&mut |id, _cap| {
    let cntrl = controller.take().unwrap();
    // TODO: We should be able to propagate errors properly on the `gui`
    //       side of things.
    Box::new(TermUi::new(id, cntrl).unwrap())
  });

  // Initially we need to trigger a render in order to have the most
  // recent data presented.
  ui.render(&renderer);

  loop {
    if let Some(term_event) = events.next() {
      // Attempt to convert the event. If we fail the reason could be
      // that it's not a key event (but a mouse event, for example) or
      // that the key is not supported. Either way we just ignore the
      // failure. The UI could not possibly react to it anyway.
      if let Ok(event) = convert(term_event?) {
        if let Some(UiEvent::Quit) = ui.handle(event) {
          break
        }
      }

      ui.render(&renderer);
    }
  }
  Ok(())
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
