// Copyright (C) 2019-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::de::Error;
use serde::de::Unexpected;
use serde::ser::SerializeTuple as _;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;

use termion::color::Color as TermColor;
use termion::color::Reset;
use termion::color::Rgb;


mod reset {
  use super::*;

  /// Deserialize a [`Reset`] value.
  pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<Reset, D::Error>
  where
    D: Deserializer<'de>,
  {
    let string = String::deserialize(deserializer)?;
    if string == "reset" {
      Ok(Reset)
    } else {
      Err(Error::invalid_value(
        Unexpected::Str(&string),
        &"the string \"reset\"",
      ))
    }
  }

  /// Serialize a [`Reset`] value.
  pub(crate) fn serialize<S>(_reset: &Reset, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str("reset")
  }
}


mod rgb {
  use super::*;

  /// Deserialize a [`Rgb`] value.
  pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<Rgb, D::Error>
  where
    D: Deserializer<'de>,
  {
    let [r, g, b] = <[u8; 3]>::deserialize(deserializer)?;
    Ok(Rgb(r, g, b))
  }

  /// Serialize a [`Rgb`] value.
  pub(crate) fn serialize<S>(rgb: &Rgb, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    let mut tuple = serializer.serialize_tuple(3)?;
    tuple.serialize_element(&rgb.0)?;
    tuple.serialize_element(&rgb.1)?;
    tuple.serialize_element(&rgb.2)?;
    tuple.end()
  }
}


#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Color {
  #[serde(with = "reset")]
  Reset(Reset),
  #[serde(with = "rgb")]
  Rgb(Rgb),
}

impl Color {
  pub fn bright_green() -> Self {
    Color::Rgb(Rgb(0x00, 0xd7, 0x00))
  }

  pub fn color0() -> Self {
    Color::Rgb(Rgb(0x00, 0x00, 0x00))
  }

  pub fn color15() -> Self {
    Color::Rgb(Rgb(0xff, 0xff, 0xff))
  }

  pub fn color197() -> Self {
    Color::Rgb(Rgb(0xff, 0x00, 0x00))
  }

  pub fn color235() -> Self {
    Color::Rgb(Rgb(0x26, 0x26, 0x26))
  }

  pub fn color240() -> Self {
    Color::Rgb(Rgb(0x58, 0x58, 0x58))
  }

  pub fn dark_white() -> Self {
    Color::Rgb(Rgb(0xda, 0xda, 0xda))
  }

  pub fn reset() -> Self {
    Color::Reset(Reset)
  }

  pub fn soft_red() -> Self {
    Color::Rgb(Rgb(0xfe, 0x0d, 0x0c))
  }

  pub fn as_term_color(&self) -> &dyn TermColor {
    match self {
      Color::Reset(c) => c,
      Color::Rgb(c) => c,
    }
  }
}

impl PartialEq for Color {
  fn eq(&self, other: &Self) -> bool {
    match self {
      Color::Reset(_) => match other {
        Color::Reset(_) => true,
        Color::Rgb(_) => false,
      },
      Color::Rgb(rgb) => match other {
        Color::Reset(_) => false,
        Color::Rgb(other_rgb) => {
          rgb.0 == other_rgb.0 && rgb.1 == other_rgb.1 && rgb.2 == other_rgb.2
        },
      },
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
  pub selected_tab_fg: Color,
  #[serde(default = "Color::color240")]
  pub selected_tab_bg: Color,
  #[serde(default = "Color::color15")]
  pub unselected_tab_fg: Color,
  #[serde(default = "Color::color235")]
  pub unselected_tab_bg: Color,
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
  #[serde(default = "Color::dark_white")]
  pub dialog_bg: Color,
  #[serde(default = "Color::color0")]
  pub dialog_fg: Color,
  #[serde(default = "Color::color15")]
  pub dialog_selected_tag_fg: Color,
  #[serde(default = "Color::color240")]
  pub dialog_selected_tag_bg: Color,
  #[serde(default = "Color::bright_green")]
  pub dialog_tag_set_fg: Color,
  #[serde(default = "Color::dark_white")]
  pub dialog_tag_set_bg: Color,
  #[serde(default = "Color::soft_red")]
  pub dialog_tag_unset_fg: Color,
  #[serde(default = "Color::dark_white")]
  pub dialog_tag_unset_bg: Color,
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
      selected_tab_fg: Color::color15(),
      selected_tab_bg: Color::color240(),
      unselected_tab_fg: Color::color15(),
      unselected_tab_bg: Color::color235(),
      unselected_task_fg: Color::color0(),
      unselected_task_bg: Color::reset(),
      selected_task_fg: Color::color15(),
      selected_task_bg: Color::color240(),
      task_not_started_fg: Color::soft_red(),
      task_not_started_bg: Color::reset(),
      task_done_fg: Color::bright_green(),
      task_done_bg: Color::reset(),
      dialog_fg: Color::color0(),
      dialog_bg: Color::dark_white(),
      dialog_selected_tag_fg: Color::color15(),
      dialog_selected_tag_bg: Color::color240(),
      dialog_tag_set_fg: Color::bright_green(),
      dialog_tag_set_bg: Color::dark_white(),
      dialog_tag_unset_fg: Color::soft_red(),
      dialog_tag_unset_bg: Color::dark_white(),
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


#[cfg(test)]
pub mod tests {
  use super::*;

  use crate::ser::backends::Backend;
  use crate::ser::backends::Json;


  #[test]
  fn color_equal() {
    let reset = Color::Reset(Reset);
    assert_eq!(reset, reset);

    let color = Color::Rgb(Rgb(127, 45, 32));
    assert_eq!(color, color);
  }

  #[test]
  fn color_unequal() {
    assert_ne!(Color::Reset(Reset), Color::Rgb(Rgb(0, 68, 11)));
    assert_ne!(Color::Rgb(Rgb(0, 68, 12)), Color::Rgb(Rgb(0, 68, 11)));
    assert_ne!(Color::Rgb(Rgb(0, 69, 12)), Color::Rgb(Rgb(0, 68, 12)));
    assert_ne!(Color::Rgb(Rgb(1, 69, 12)), Color::Rgb(Rgb(0, 69, 12)));
  }

  /// Check that we can serialize the [`Color::Reset`] variant and
  /// deserialize it back.
  #[test]
  fn serialize_deserialize_reset() {
    let reset = Color::Reset(Reset);
    let serialized = Json::serialize(&reset).unwrap();
    assert_eq!(serialized, b"\"reset\"");

    let deserialized = <Json as Backend<Color>>::deserialize(&serialized).unwrap();
    assert_eq!(deserialized, reset);
  }

  /// Check that we can serialize the [`Color::Rgb`] variant and
  /// deserialize it back.
  #[test]
  fn serialize_deserialize_rgb() {
    let rgb = Color::Rgb(Rgb(1, 2, 3));
    let serialized = Json::serialize(&rgb).unwrap();
    let expected = br#"[
  1,
  2,
  3
]"#;
    assert_eq!(serialized, expected);

    let deserialized = <Json as Backend<Color>>::deserialize(&serialized).unwrap();
    assert_eq!(deserialized, rgb);
  }

  /// Check that we can serialize and then deserialize again the default
  /// representation of the [`Colors`] type.
  #[test]
  fn serialize_deserialize_colors() {
    let colors = Colors::default();
    let serialized = Json::serialize(&colors).unwrap();
    let deserialized = <Json as Backend<Colors>>::deserialize(&serialized).unwrap();
    assert_eq!(deserialized, colors);
  }
}
