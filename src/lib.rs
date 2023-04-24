// Copyright (C) 2017-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

// We basically deny most lints that "warn" by default, except for
// those that may change in incompatible ways the future. We want to
// avoid build breakages when upgrading to new Rust versions.
#![warn(
  bad_style,
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
  while_true,
  clippy::dbg_macro,
  rustdoc::broken_intra_doc_links
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

pub mod cap;
mod colors;
mod db;
mod id;
mod ops;
mod position;
mod resize;
mod ser;
pub mod state;
mod tags;
mod tasks;
#[cfg(any(test, feature = "test"))]
pub mod test;
mod ui;
mod view;

use std::env::args_os;
use std::ffi::OsString;
use std::fs::create_dir_all;
use std::fs::remove_file;
use std::fs::File;
use std::io::stdin;
use std::io::stdout;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Result as IoResult;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::thread;

use anyhow::anyhow;
use anyhow::bail;
use anyhow::Context as _;
use anyhow::Result;

#[cfg(feature = "coredump")]
use cdump::register_panic_handler;

use dirs::cache_dir;
use dirs::config_dir;

use termion::event::Event as TermEvent;
use termion::event::Key;
use termion::input::TermReadEventsAndRaw;
use termion::raw::IntoRawMode;
use termion::screen::IntoAlternateScreen as _;

use tokio::runtime::Builder;

use gui::Renderer;
use gui::Ui;

use crate::cap::DirCap;
use crate::resize::receive_window_resizes;
use crate::state::TaskState;
use crate::state::UiState;
use crate::ui::event::Event as UiEvent;
use crate::ui::message::Message;
use crate::ui::term_renderer::TermRenderer;
use crate::ui::termui::TermUi;
use crate::ui::termui::TermUiData;


/// A tuple of (directory path, file name) representing the path to a
/// file.
type FilePath = (PathBuf, OsString);


/// An event to be handled by the program.
#[derive(Clone, Debug)]
pub enum Event {
  /// A key that has been received, including the raw input data.
  Key(Key, Vec<u8>),
  /// The window has been resized.
  Resize,
}


/// Retrieve the path to the program's lock file.
fn lock_file() -> Result<PathBuf> {
  let path = cache_dir()
    .ok_or_else(|| anyhow!("unable to determine cache directory"))?
    .join("notnow.lock");
  Ok(path)
}

/// Retrieve the path to the UI's configuration file, in the form of a
/// (directory path, file name) tuple.
fn ui_config() -> Result<FilePath> {
  let config_dir = config_dir()
    .ok_or_else(|| anyhow!("unable to determine config directory"))?
    .join("notnow");
  let config_file = OsString::from("notnow.json");

  Ok((config_dir, config_file))
}

/// Retrieve the path to the program's task directory.
fn tasks_root() -> Result<PathBuf> {
  Ok(
    config_dir()
      .ok_or_else(|| anyhow!("unable to determine config directory"))?
      .join("notnow")
      .join("tasks"),
  )
}

/// Instantiate a key receiver thread and have it send key events through the given channel.
fn receive_keys<R>(stdin: R, send_event: Sender<IoResult<Event>>)
where
  R: Read + Send + 'static,
{
  thread::spawn(move || {
    let events = stdin.events_and_raw();
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
  recv_event: &Receiver<IoResult<Event>>,
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
pub async fn run_prog<R, W>(in_: R, out: W, ui_config: FilePath, tasks_root: PathBuf) -> Result<()>
where
  R: Read + Send + 'static,
  W: Write,
{
  let task_state = TaskState::load(&tasks_root)
    .await
    .context("failed to load task state")?;
  let ui_file = ui_config.0.join(&ui_config.1);
  let ui_state = UiState::load(&ui_file, &task_state)
    .await
    .context("failed to load UI state")?;
  let screen = out
    .into_alternate_screen()?
    .into_raw_mode()
    .context("failed to switch program output to raw mode")?;
  let colors = ui_state.colors.get().unwrap_or_default();
  let renderer =
    TermRenderer::new(screen, colors).context("failed to instantiate terminal based renderer")?;

  let ui_dir_cap = DirCap::for_dir(ui_config.0).await?;
  let ui_config = ui_config.1;

  let tasks_root_cap = DirCap::for_dir(tasks_root).await?;

  let (ui, _) = Ui::new(
    || {
      Box::new(TermUiData::new(
        (ui_dir_cap, ui_config),
        tasks_root_cap,
        task_state,
      ))
    },
    |id, cap| Box::new(TermUi::new(id, cap, ui_state)),
  );

  let (send_event, recv_event) = channel();
  receive_window_resizes(send_event.clone())
    .context("failed to instantiate infrastructure for handling window resize events")?;
  receive_keys(in_, send_event);

  // Initially we need to trigger a render in order to have the most
  // recent data presented.
  ui.render(&renderer);

  run_loop(ui, &renderer, &recv_event).await
}

/// Run a function after attempting to create a lock file and remove it
/// once the function has returned.
fn with_lockfile<F>(lock_file: &Path, force: bool, f: F) -> Result<()>
where
  F: FnOnce() -> Result<()>,
{
  if let Some(dir) = lock_file.parent() {
    let () = create_dir_all(dir)
      .with_context(|| format!("failed to create directory {}", dir.display()))?;
  }

  if force {
    let _file = File::options()
      .create(true)
      .write(true)
      .open(lock_file)
      .with_context(|| {
        format!(
          "failed to take ownership of lock file {}",
          lock_file.display()
        )
      });
  } else {
    let result = File::options().create_new(true).write(true).open(lock_file);
    if matches!(&result, Err(err) if err.kind() == ErrorKind::AlreadyExists) {
      eprintln!(
        "lock file {} already present; is another program instance running?",
        lock_file.display()
      );
      eprintln!("re-run with --force/-f if you are sure that the file is stale");
    }
    let _file =
      result.with_context(|| format!("failed to create lock file {}", lock_file.display()))?;
  }

  let result = f();

  // Note that one error scenario when removing the lock file is that it
  // was already removed by a concurrently running instance. That can
  // only happen if the user wrongly specified the --force/-f flag. We
  // could special case that here, but it just does not seem worth it,
  // so just handle it like any other error.
  match (result, remove_file(lock_file)) {
    (Ok(()), Ok(())) => Ok(()),
    (Ok(()), r @ Err(_)) => {
      r.with_context(|| format!("failed to remove lock file {}", lock_file.display()))
    },
    (r @ Err(_), Ok(())) => r,
    (r @ Err(_), Err(_)) => {
      eprintln!("failed to remove lock file {}", lock_file.display());
      r
    },
  }
}

/// Run an instance of the program in the default configuration.
fn run_now() -> Result<()> {
  let ui_config = ui_config()?;
  let tasks_root = tasks_root()?;
  let rt = Builder::new_current_thread()
    .build()
    .context("failed to instantiate async runtime")?;

  let stdin = stdin();
  let stdout = stdout();
  let future = run_prog(stdin, stdout.lock(), ui_config, tasks_root);
  rt.block_on(future)
}

/// Parse the arguments and run the program.
fn run_with_args(lock_file: &Path) -> Result<()> {
  let mut it = args_os();
  match it.len() {
    0 | 1 => with_lockfile(lock_file, false, run_now),
    2 if it.any(|arg| &arg == "--force" || &arg == "-f") => with_lockfile(lock_file, true, run_now),
    2 if it.any(|arg| &arg == "--version" || &arg == "-V") => {
      println!("{} {}", env!("CARGO_CRATE_NAME"), env!("NOTNOW_VERSION"));
      Ok(())
    },
    _ => bail!("encountered unsupported number of program arguments"),
  }
}

fn run_with_result() -> Result<()> {
  #[cfg(feature = "coredump")]
  {
    let () = register_panic_handler().or_else(|(ctx, err)| {
      Err(err)
        .context(ctx)
        .context("failed to register core dump panic handler")
    })?;
  }

  run_with_args(&lock_file()?)
}

/// Run the program and handle errors.
pub fn run() -> i32 {
  match run_with_result() {
    Ok(_) => 0,
    Err(err) => {
      eprintln!("{:?}", err);
      1
    },
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use tempfile::NamedTempFile;


  /// Check that `with_lockfile` behaves correctly in the presence of a
  /// lock file.
  #[test]
  fn lock_file_present() {
    let lock_file = NamedTempFile::new().unwrap();
    let force = false;
    let error = with_lockfile(lock_file.path(), force, || Ok(())).unwrap_err();
    assert!(error
      .to_string()
      .contains(&lock_file.path().display().to_string()));
  }

  /// Check that `with_lockfile` behaves correctly in the presence of a
  /// lock file when asked to forcefully acquire.
  #[test]
  fn lock_file_present_force() {
    let lock_file = NamedTempFile::new().unwrap();
    let force = true;
    let () = with_lockfile(lock_file.path(), force, || Ok(())).unwrap();

    // The lock file should have been removed.
    assert!(!lock_file.path().exists());
  }

  /// Check that `with_lockfile` behaves correctly when a lock file is
  /// present and the called function returns an error.
  #[test]
  fn lock_file_error_when_present() {
    let lock_file = NamedTempFile::new().unwrap();
    let force = false;
    let error = with_lockfile(lock_file.path(), force, || bail!("42")).unwrap_err();
    assert!(error
      .to_string()
      .contains(&lock_file.path().display().to_string()));
  }

  /// Check that `with_lockfile` behaves correctly if no lock file is
  /// present.
  #[test]
  fn lock_file_not_present() {
    let lock_file_path = {
      let lock_file = NamedTempFile::new().unwrap();
      lock_file.path().to_path_buf()
    };

    let force = false;
    let () = with_lockfile(&lock_file_path, force, || Ok(())).unwrap();
  }

  /// Check that `with_lockfile` behaves correctly when no lock file is
  /// present and the called function returns an error.
  #[test]
  fn lock_file_error_when_not_present() {
    let lock_file_path = {
      let lock_file = NamedTempFile::new().unwrap();
      lock_file.path().to_path_buf()
    };

    let force = false;
    let error = with_lockfile(&lock_file_path, force, || bail!("42")).unwrap_err();
    assert_eq!(&error.to_string(), "42");
  }
}
