// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module containing functionality for the different supported
//! serialization and deserialization backends.

mod backend;
mod json;

pub use backend::Backend;
pub use json::Json;
