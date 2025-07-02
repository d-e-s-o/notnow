// Copyright (C) 2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later


#[cfg(feature = "log")]
#[macro_use]
#[expect(unused_imports, clippy::module_inception)]
mod log {
  use std::env::temp_dir;
  use std::fs::File;
  use std::panic::set_hook;
  use std::panic::take_hook;
  use std::path::Path;

  use anyhow::Context as _;
  use anyhow::Result;

  use tracing::subscriber::set_global_default as set_global_subscriber;
  use tracing::Level;
  use tracing::Subscriber;
  use tracing_subscriber::fmt::time::SystemTime;
  use tracing_subscriber::FmtSubscriber;

  pub(crate) use tracing::debug;
  pub(crate) use tracing::error;
  pub(crate) use tracing::info;
  pub(crate) use tracing::instrument;
  pub(crate) use tracing::trace;
  pub(crate) use tracing::warn;

  /// Create a `Subscriber` that logs trace events to a file at the
  /// given path.
  fn make_subscriber(log_path: &Path) -> Result<impl Subscriber + Send + Sync + 'static> {
    let log_file = File::options()
      .create(true)
      .truncate(true)
      .read(false)
      .write(true)
      .open(log_path)
      .with_context(|| format!("failed to open log file {}", log_path.display()))?;

    let subscriber = FmtSubscriber::builder()
      .with_ansi(true)
      .with_level(true)
      .with_target(false)
      .with_max_level(Level::TRACE)
      .with_timer(SystemTime)
      .with_writer(log_file)
      .finish();

    Ok(subscriber)
  }

  pub(crate) fn init() -> Result<()> {
    let log_file = temp_dir().join("notnow.log");
    let subscriber = make_subscriber(&log_file).context("failed to create tracing subscriber")?;
    let () = set_global_subscriber(subscriber).context("failed to set tracing subscriber")?;

    let default_panic = take_hook();
    let () = set_hook(Box::new(move |panic_info| {
      error!("Panic: {panic_info}");
      default_panic(panic_info);
    }));
    Ok(())
  }
}

#[cfg(not(feature = "log"))]
#[macro_use]
#[expect(unused_imports, clippy::module_inception)]
mod log {
  use anyhow::Result;

  #[expect(unused_macros)]
  macro_rules! debug {
    ($($args:tt)*) => {};
  }
  pub(crate) use debug;
  pub(crate) use debug as error;
  pub(crate) use debug as info;
  pub(crate) use debug as trace;
  pub(crate) use debug as warn;

  pub(crate) fn init() -> Result<()> {
    Ok(())
  }
}

pub(crate) use log::*;
