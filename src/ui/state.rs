// Copyright (C) 2023 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use anyhow::Context as _;
use anyhow::Result;

use crate::cap::FileCap;
use crate::ser::backends::Json;
use crate::ser::state::UiState as SerUiState;
use crate::ser::ToSerde;
use crate::state::load_state_from_file;
use crate::state::save_state_to_file;


/// A struct encapsulating the UI's "volatile" state.
#[derive(Debug, Default)]
pub struct State {
  /// The index of the task currently selected on each view, indexed by
  /// view.
  pub selected_tasks: Vec<Option<usize>>,
  /// The currently selected `View`.
  pub selected_view: Option<usize>,
}

impl State {
  /// Load a `State` object from a file.
  pub async fn load(state_path: &Path) -> Result<Self> {
    let state = load_state_from_file::<Json, SerUiState>(state_path)
      .await
      .with_context(|| format!("failed to load UI state from {}", state_path.display()))?
      .unwrap_or_default();

    Ok(Self::with_serde(state))
  }

  /// Create a `State` object from a serialized one.
  pub fn with_serde(state: SerUiState) -> Self {
    Self {
      selected_tasks: state.selected_tasks,
      selected_view: state.selected_view,
    }
  }

  /// Persist the state into a file.
  pub async fn save(&self, file_cap: &mut FileCap<'_>) -> Result<()> {
    save_state_to_file::<Json, _>(file_cap, &self.to_serde()).await
  }
}

impl ToSerde for State {
  type Output = SerUiState;

  /// Convert this object into a serializable one.
  fn to_serde(&self) -> Self::Output {
    let state = SerUiState {
      selected_tasks: self.selected_tasks.clone(),
      selected_view: self.selected_view,
    };
    state
  }
}
