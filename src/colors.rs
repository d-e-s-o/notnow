// Copyright (C) 2019,2021 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::marker::PhantomData;

use serde::de::EnumAccess;
use serde::de::Error;
use serde::de::SeqAccess;
use serde::de::Unexpected;
use serde::de::VariantAccess;
use serde::de::Visitor;
use serde::ser::SerializeTupleVariant;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;

use termion::color::Color as TermColor;
use termion::color::Reset;
use termion::color::Rgb;


#[derive(Clone, Copy, Debug)]
pub enum Color {
  Reset(Reset),
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

impl Serialize for Color {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    match self {
      Color::Reset(_) => serializer.serialize_unit_variant("Color", 0, "reset"),
      Color::Rgb(c) => {
        let mut tuple = serializer.serialize_tuple_variant("Color", 1, "rgb", 3)?;
        tuple.serialize_field(&c.0)?;
        tuple.serialize_field(&c.1)?;
        tuple.serialize_field(&c.2)?;
        tuple.end()
      },
    }
  }
}

impl<'de> Deserialize<'de> for Color {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    deserializer.deserialize_enum(
      "Color",
      VARIANTS,
      ColorVisitor {
        marker: PhantomData,
      },
    )
  }
}


const VARIANTS: &[&str] = &["reset", "rgb"];


/// A helper enum for deserializing a `Color`.
enum ColorEnum {
  Reset,
  Rgb,
}

impl<'de> Deserialize<'de> for ColorEnum {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    deserializer.deserialize_identifier(VariantVisitor)
  }
}


/// A visitor for the `Color` enum.
struct ColorVisitor<'de> {
  marker: PhantomData<&'de Color>,
}

impl<'de> Visitor<'de> for ColorVisitor<'de> {
  type Value = Color;

  fn expecting(&self, formatter: &mut Formatter<'_>) -> FmtResult {
    formatter.write_str("enum Color")
  }

  fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
  where
    A: EnumAccess<'de>,
  {
    EnumAccess::variant(data).and_then(|value| match value {
      (ColorEnum::Reset, variant) => {
        VariantAccess::unit_variant(variant).map(|_| Color::Reset(Reset))
      },
      (ColorEnum::Rgb, variant) => VariantAccess::tuple_variant(
        variant,
        3,
        RgbVisitor {
          marker: PhantomData,
        },
      ),
    })
  }
}


/// A visitor for the individual variants of the `Color` enum.
struct VariantVisitor;

impl<'de> Visitor<'de> for VariantVisitor {
  type Value = ColorEnum;

  fn expecting(&self, formatter: &mut Formatter<'_>) -> FmtResult {
    formatter.write_str("variant identifier")
  }

  fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
  where
    E: Error,
  {
    match value {
      0 => Ok(ColorEnum::Reset),
      1 => Ok(ColorEnum::Rgb),
      _ => Err(Error::invalid_value(
        Unexpected::Unsigned(value),
        &"variant index 0 <= i < 2",
      )),
    }
  }

  fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
  where
    E: Error,
  {
    match value {
      "reset" => Ok(ColorEnum::Reset),
      "rgb" => Ok(ColorEnum::Rgb),
      _ => Err(Error::unknown_variant(value, VARIANTS)),
    }
  }

  fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
  where
    E: Error,
  {
    match value {
      b"reset" => Ok(ColorEnum::Reset),
      b"rgb" => Ok(ColorEnum::Rgb),
      _ => {
        let value = &String::from_utf8_lossy(value);
        Err(Error::unknown_variant(value, VARIANTS))
      },
    }
  }
}


/// A visitor for the `Color::Rgb` variant.
struct RgbVisitor<'de> {
  marker: PhantomData<&'de Color>,
}

impl<'de> Visitor<'de> for RgbVisitor<'de> {
  type Value = Color;

  fn expecting(&self, formatter: &mut Formatter<'_>) -> FmtResult {
    formatter.write_str("tuple variant Color::Rgb")
  }

  fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
  where
    A: SeqAccess<'de>,
  {
    let r = match match SeqAccess::next_element::<u8>(&mut seq) {
      Ok(value) => value,
      Err(err) => return Err(err),
    } {
      Some(value) => value,
      None => {
        return Err(Error::invalid_length(
          0,
          &"tuple variant Color::Rgb with 3 elements",
        ))
      },
    };

    let g = match match SeqAccess::next_element::<u8>(&mut seq) {
      Ok(value) => value,
      Err(err) => return Err(err),
    } {
      Some(value) => value,
      None => {
        return Err(Error::invalid_length(
          1,
          &"tuple variant Color::Rgb with 3 elements",
        ))
      },
    };

    let b = match match SeqAccess::next_element::<u8>(&mut seq) {
      Ok(value) => value,
      Err(err) => return Err(err),
    } {
      Some(value) => value,
      None => {
        return Err(Error::invalid_length(
          2,
          &"tuple variant Color::Rgb with 3 elements",
        ))
      },
    };

    Ok(Color::Rgb(Rgb(r, g, b)))
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

  use serde_json::from_str as from_json;
  use serde_json::to_string as to_json;


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

  #[test]
  fn serialize_deserialize_reset() {
    let reset = Color::Reset(Reset);
    let serialized = to_json(&reset).unwrap();
    assert_eq!(serialized, "\"reset\"");

    let deserialized = from_json::<Color>(&serialized).unwrap();
    assert_eq!(deserialized, reset);
  }

  #[test]
  fn serialize_deserialize_rgb() {
    let rgb = Color::Rgb(Rgb(1, 2, 3));
    let serialized = to_json(&rgb).unwrap();
    assert_eq!(serialized, "{\"rgb\":[1,2,3]}");

    let deserialized = from_json::<Color>(&serialized).unwrap();
    assert_eq!(deserialized, rgb);
  }
}
