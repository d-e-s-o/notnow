// colors.rs

// *************************************************************************
// * Copyright (C) 2019 Daniel Mueller (deso@posteo.net)                   *
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

use serde::Deserialize;
use serde::Serialize;

use termion::color::Color as TermColor;
use termion::color::Reset;
use termion::color::Rgb;


#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Color {
  Reset,
  Rgb(u8, u8, u8),
}

impl Color {
  pub fn bright_green() -> Self {
    Color::Rgb(0x00, 0xd7, 0x00)
  }

  pub fn color0() -> Self {
    Color::Rgb(0x00, 0x00, 0x00)
  }

  pub fn color15() -> Self {
    Color::Rgb(0xff, 0xff, 0xff)
  }

  pub fn color197() -> Self {
    Color::Rgb(0xff, 0x00, 0x00)
  }

  pub fn color235() -> Self {
    Color::Rgb(0x26, 0x26, 0x26)
  }

  pub fn color240() -> Self {
    Color::Rgb(0x58, 0x58, 0x58)
  }

  pub fn reset() -> Self {
    Color::Reset
  }

  pub fn soft_red() -> Self {
    Color::Rgb(0xfe, 0x0d, 0x0c)
  }

  pub fn to_term_color(self) -> Box<dyn TermColor> {
    match self {
      Color::Reset => Box::new(Reset),
      Color::Rgb(r, g, b) => Box::new(Rgb(r, g, b)),
    }
  }
}


#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Colors {
  #[serde(default = "Color::color0")]
  pub more_tasks_fg: Color,
  #[serde(default = "Color::bright_green")]
  pub more_tasks_bg: Color,
  #[serde(default = "Color::color15")]
  pub selected_query_fg: Color,
  #[serde(default = "Color::color240")]
  pub selected_query_bg: Color,
  #[serde(default = "Color::color15")]
  pub unselected_query_fg: Color,
  #[serde(default = "Color::color235")]
  pub unselected_query_bg: Color,
  #[serde(default = "Color::color0")]
  pub unselected_task_fg: Color,
  #[serde(default = "Color::reset")]
  pub unselected_task_bg: Color,
  #[serde(default = "Color::color15")]
  pub selected_task_fg: Color,
  #[serde(default = "Color::color240")]
  pub selected_task_bg: Color,
  #[serde(default = "Color::soft_red")]
  pub task_not_started_fg: Color,
  #[serde(default = "Color::reset")]
  pub task_not_started_bg: Color,
  #[serde(default = "Color::bright_green")]
  pub task_done_fg: Color,
  #[serde(default = "Color::reset")]
  pub task_done_bg: Color,
  #[serde(default = "Color::color0")]
  pub in_out_success_fg: Color,
  #[serde(default = "Color::bright_green")]
  pub in_out_success_bg: Color,
  #[serde(default = "Color::color15")]
  pub in_out_status_fg: Color,
  #[serde(default = "Color::color0")]
  pub in_out_status_bg: Color,
  #[serde(default = "Color::color0")]
  pub in_out_error_fg: Color,
  #[serde(default = "Color::color197")]
  pub in_out_error_bg: Color,
  #[serde(default = "Color::reset")]
  pub in_out_string_fg: Color,
  #[serde(default = "Color::reset")]
  pub in_out_string_bg: Color,
}

impl Default for Colors {
  fn default() -> Self {
    Self {
      more_tasks_fg: Color::color0(),
      more_tasks_bg: Color::bright_green(),
      selected_query_fg: Color::color15(),
      selected_query_bg: Color::color240(),
      unselected_query_fg: Color::color15(),
      unselected_query_bg: Color::color235(),
      unselected_task_fg: Color::color0(),
      unselected_task_bg: Color::reset(),
      selected_task_fg: Color::color15(),
      selected_task_bg: Color::color240(),
      task_not_started_fg: Color::soft_red(),
      task_not_started_bg: Color::reset(),
      task_done_fg: Color::bright_green(),
      task_done_bg: Color::reset(),
      in_out_success_fg: Color::color0(),
      in_out_success_bg: Color::bright_green(),
      in_out_status_fg: Color::color15(),
      in_out_status_bg: Color::color0(),
      in_out_error_fg: Color::color0(),
      in_out_error_bg: Color::color197(),
      in_out_string_fg: Color::reset(),
      in_out_string_bg: Color::reset(),
    }
  }
}
