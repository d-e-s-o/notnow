// term_renderer.rs

// *************************************************************************
// * Copyright (C) 2018 Daniel Mueller (deso@posteo.net)                   *
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

use std::any::Any;
use std::cell::RefCell;
use std::io::BufWriter;
use std::io::Result;
use std::io::Write;

use termion::clear::All;
use termion::color::Bg;
use termion::color::Color;
use termion::color::Fg;
use termion::color::Reset;
use termion::color::Rgb;
use termion::cursor::Goto;
use termion::cursor::Hide;
use termion::cursor::Show;
use termion::terminal_size;

use gui::BBox;
use gui::Renderer;

use in_out::InOut;
use in_out::InOutArea;
use termui::TermUi;

const MAIN_MARGIN_X: u16 = 3;
const MAIN_MARGIN_Y: u16 = 2;
const TASK_SPACE: u16 = 2;

const SAVED_TEXT: &str = " Saved ";
const ERROR_TEXT: &str = " Error ";
const INPUT_TEXT: &str = " > ";

// TODO: Make the colors run time configurable at some point.
/// Color 0.
const UNSELECTED_TASK_FG: &Rgb = &Rgb(0x00, 0x00, 0x00);
/// The terminal default background.
const UNSELECTED_TASK_BG: &Reset = &Reset;
/// Color 15.
const SELECTED_TASK_FG: &Rgb = &Rgb(0xff, 0xff, 0xff);
/// Color 240.
const SELECTED_TASK_BG: &Rgb = &Rgb(0x58, 0x58, 0x58);
/// Color 0.
const IN_OUT_SUCCESS_FG: &Rgb = &Rgb(0x00, 0x00, 0x00);
/// Color 40.
const IN_OUT_SUCCESS_BG: &Rgb = &Rgb(0x00, 0xd7, 0x00);
/// Color 0.
const IN_OUT_ERROR_FG: &Rgb = &Rgb(0x00, 0x00, 0x00);
/// Color 197.
const IN_OUT_ERROR_BG: &Rgb = &Rgb(0xff, 0x00, 0x00);
/// Color 0.
const IN_OUT_STRING_FG: &Rgb = &Rgb(0x00, 0x00, 0x00);
/// The terminal default background.
const IN_OUT_STRING_BG: &Reset = &Reset;


/// Sanitize an offset.
fn sanitize_offset(offset: usize, selection: usize, limit: usize) -> usize {
  if selection <= offset {
    selection
  } else if selection > offset + (limit - 1) {
    selection - (limit - 1)
  } else {
    offset
  }
}


pub struct TermRenderer<W>
where
  W: Write,
{
  writer: RefCell<BufWriter<W>>,
}

impl<W> TermRenderer<W>
where
  W: Write,
{
  /// Create a new `TermRenderer` object.
  pub fn new(writer: W) -> Result<Self> {
    // Compared to termbox termion suffers from flickering when clearing
    // the entire screen as it lacks any double buffering capabilities
    // and uses an escape sequence for the clearing. One proposed
    // solution is to use a io::BufWriter. Tests have shown that it does
    // not change much but conceptually it makes sense to use it
    // nevertheless -- so this is what we do. For a broader discussion
    // of this issue see https://github.com/ticki/termion/issues/105.
    let writer = RefCell::new(BufWriter::new(writer));

    write!(writer.borrow_mut(), "{}{}", Hide, All)?;

    Ok(TermRenderer {
      writer: writer,
    })
  }

  /// Retrieve the number of tasks that fit in the given `BBox`.
  fn displayable_tasks(&self, bbox: BBox) -> usize {
    ((bbox.h - MAIN_MARGIN_Y) / TASK_SPACE) as usize
  }

  /// Render a `TermUi`.
  fn render_term_ui(&self, ui: &TermUi, bbox: BBox) -> Result<BBox> {
    let x = bbox.x + MAIN_MARGIN_X;
    let mut y = bbox.y + MAIN_MARGIN_Y;

    let iter = ui.controller().tasks();
    let limit = self.displayable_tasks(bbox);
    let selection = ui.selection();
    let offset = sanitize_offset(ui.offset(), selection, limit);

    for (i, task) in iter.enumerate().skip(offset) {
      if i >= offset + limit {
        break
      }

      let (fg, bg) = if i == selection {
        (SELECTED_TASK_FG as &Color, SELECTED_TASK_BG as &Color)
      } else {
        (UNSELECTED_TASK_FG as &Color, UNSELECTED_TASK_BG as &Color)
      };

      write!(
        self.writer.borrow_mut(),
        "{}{}{}{}",
        Goto(x + 1, y + 1),
        Fg(fg),
        Bg(bg),
        task.summary
      )?;
      y += TASK_SPACE;
    }

    // We need to adjust the offset we use in order to be able to give
    // the correct impression of a sliding selection window.
    ui.reoffset(offset);
    Ok(bbox)
  }

  /// Render an `InOutArea`.
  fn render_input_output(&self, in_out: &InOutArea, bbox: BBox) -> Result<BBox> {
    let x = bbox.x;
    let y = bbox.y + bbox.h;

    let (prefix, fg, bg, string) = match in_out.state() {
      InOut::Saved => (Some(SAVED_TEXT), IN_OUT_SUCCESS_FG, IN_OUT_SUCCESS_BG, None),
      InOut::Error(ref e) => (Some(ERROR_TEXT), IN_OUT_ERROR_FG, IN_OUT_ERROR_BG, Some(e)),
      InOut::Input(ref s) => (Some(INPUT_TEXT), IN_OUT_SUCCESS_FG, IN_OUT_SUCCESS_BG, Some(s)),
      InOut::Clear => return Ok(Default::default()),
    };

    write!(self.writer.borrow_mut(), "{}", Goto(x + 1, y + 1))?;

    if let Some(prefix) = prefix {
      write!(self.writer.borrow_mut(), "{}{}{}", Fg(*fg), Bg(*bg), prefix)?;
    }
    if let Some(string) = string {
      write!(
        self.writer.borrow_mut(),
        "{}{} {}",
        Fg(*IN_OUT_STRING_FG),
        Bg(*IN_OUT_STRING_BG),
        string
      )?;
    }
    Ok(Default::default())
  }
}

impl<W> Renderer for TermRenderer<W>
where
  W: Write,
{
  fn renderable_area(&self) -> BBox {
    match terminal_size() {
      Ok((w, h)) => {
        BBox {
          x: 0,
          y: 0,
          w: w,
          h: h,
        }
      },
      Err(e) => panic!("Retrieving terminal size failed: {}", e),
    }
  }

  fn pre_render(&self) {
    let err = write!(
      self.writer.borrow_mut(),
      "{}{}{}",
      Fg(Reset),
      Bg(Reset),
      All
    );
    if let Err(e) = err {
      panic!("Pre-render failed: {}", e);
    }
  }

  fn render(&self, widget: &Any, bbox: BBox) -> BBox {
    let result;

    if let Some(ui) = widget.downcast_ref::<TermUi>() {
      result = self.render_term_ui(ui, bbox)
    } else if let Some(in_out) = widget.downcast_ref::<InOutArea>() {
      result = self.render_input_output(in_out, bbox);
    } else {
      panic!("Widget {:?} is unknown to the renderer", widget)
    }

    match result {
      Ok(b) => b,
      Err(e) => panic!("Rendering failed: {}", e),
    }
  }

  fn post_render(&self) {
    let err = self.writer.borrow_mut().flush();
    if let Err(e) = err {
      panic!("Post-render failed: {}", e);
    }
  }
}

impl<W> Drop for TermRenderer<W>
where
  W: Write,
{
  fn drop(&mut self) {
    let _ = write!(self.writer.borrow_mut(), "{}", Show);
  }
}
