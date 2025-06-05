// Copyright (C) 2022-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Build script for notnow.

use anyhow::Result;

use grev::git_revision_auto;


fn main() -> Result<()> {
  let dir = env!("CARGO_MANIFEST_DIR");
  if let Some(git_rev) = git_revision_auto(dir)? {
    println!(
      "cargo:rustc-env=VERSION={} ({})",
      env!("CARGO_PKG_VERSION"),
      git_rev
    );
  } else {
    println!("cargo:rustc-env=VERSION={}", env!("CARGO_PKG_VERSION"));
  }
  Ok(())
}
