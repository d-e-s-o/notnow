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

use std::cell::Cell;
use std::cell::RefCell;
use std::io::BufWriter;
use std::io::Error;
use std::io::Result;
use std::io::Write;
use std::iter::repeat;

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
use gui::Cap;
use gui::Object;
use gui::Renderer;
use gui::Widget;

use in_out::InOut;
use in_out::InOutArea;
use tab_bar::TabBar;
use task_list_box::TaskListBox;
use termui::TermUi;

const MAIN_MARGIN_X: u16 = 3;
const MAIN_MARGIN_Y: u16 = 2;
const TASK_SPACE: u16 = 2;
const TAB_TITLE_WIDTH: u16 = 30;

const SAVED_TEXT: &str = " Saved ";
const ERROR_TEXT: &str = " Error ";
const INPUT_TEXT: &str = " > ";

// TODO: Make the colors run time configurable at some point.
/// Color 15.
const SELECTED_QUERY_FG: Rgb = Rgb(0xff, 0xff, 0xff);
/// Color 240.
const SELECTED_QUERY_BG: Rgb = Rgb(0x58, 0x58, 0x58);
/// Color 15.
const UNSELECTED_QUERY_FG: Rgb = Rgb(0xff, 0xff, 0xff);
/// Color 235.
const UNSELECTED_QUERY_BG: Rgb = Rgb(0x26, 0x26, 0x26);
/// Color 0.
const UNSELECTED_TASK_FG: Rgb = Rgb(0x00, 0x00, 0x00);
/// The terminal default background.
const UNSELECTED_TASK_BG: Reset = Reset;
/// Color 15.
const SELECTED_TASK_FG: Rgb = Rgb(0xff, 0xff, 0xff);
/// Color 240.
const SELECTED_TASK_BG: Rgb = Rgb(0x58, 0x58, 0x58);
/// Soft red.
const TASK_NOT_STARTED_FG: Rgb = Rgb(0xfe, 0x0d, 0x0c);
/// Color 15.
const TASK_NOT_STARTED_BG: Reset = Reset;
/// Bright green.
const TASK_DONE_FG: Rgb = Rgb(0x00, 0xd7, 0x00);
/// Color 15.
const TASK_DONE_BG: Reset = Reset;
/// Color 0.
const IN_OUT_SUCCESS_FG: Rgb = Rgb(0x00, 0x00, 0x00);
/// Color 40.
const IN_OUT_SUCCESS_BG: Rgb = Rgb(0x00, 0xd7, 0x00);
/// Color 0.
const IN_OUT_ERROR_FG: Rgb = Rgb(0x00, 0x00, 0x00);
/// Color 197.
const IN_OUT_ERROR_BG: Rgb = Rgb(0xff, 0x00, 0x00);
/// Color 0.
const IN_OUT_STRING_FG: Rgb = Rgb(0x00, 0x00, 0x00);
/// The terminal default background.
const IN_OUT_STRING_BG: Reset = Reset;


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

/// Align string centrally in the given `width` or cut it off if it is too long.
fn align_center(string: impl Into<String>, width: usize) -> String {
  let mut string = string.into();
  let length = string.len();

  if length > width {
    // Note: May underflow if width < 3. That's not really a supported
    //       use case, though, so we ignore it here.
    string.replace_range(width - 3..length, "...");
  } else if length < width {
    let pad_right = (width - length) / 2;
    let pad_left = width - length - pad_right;

    let pad_right = repeat(" ").take(pad_right).collect::<String>();
    let pad_left = repeat(" ").take(pad_left).collect::<String>();

    string.insert_str(0, &pad_right);
    string.push_str(&pad_left);
  }
  string
}

/// Clip a string according to the active bounding box.
fn clip(x: u16, y: u16, string: &str, bbox: BBox) -> &str {
  let w = bbox.w;
  let h = bbox.h;

  if y < h {
    if x + string.len() as u16 >= w {
      &string[..(w - x) as usize]
    } else {
      string
    }
  } else {
    ""
  }
}


/// A writer that clips writes according to a bounding box.
struct ClippingWriter<W>
where
  W: Write,
{
  writer: RefCell<W>,
  bbox: Cell<BBox>,
}

impl<W> ClippingWriter<W>
where
  W: Write,
{
  /// Create a new `ClippingWriter` object.
  fn new(writer: W) -> Self {
    ClippingWriter {
      writer: RefCell::new(writer),
      bbox: Default::default(),
    }
  }

  /// Set the `BBox` to restrict the rendering area to.
  fn restrict(&self, bbox: BBox) {
    self.bbox.set(bbox)
  }

  /// Write a string to the terminal.
  fn write<F, B, S>(&self, x: u16, y: u16, fg: F, bg: B, string: S) -> Result<()>
  where
    F: Color,
    B: Color,
    S: AsRef<str>,
  {
    let string = clip(x, y, string.as_ref(), self.bbox.get());
    if !string.is_empty() {
      // Specified coordinates are always relative to the bounding box
      // set. Termion works with an origin at (1,1). We fudge that by
      // adjusting all coordinates accordingly.
      let x = self.bbox.get().x + x + 1;
      let y = self.bbox.get().y + y + 1;

      write!(
        self.writer.borrow_mut(),
        "{}{}{}{}",
        Goto(x, y),
        Fg(fg),
        Bg(bg),
        string,
      )?
    }
    Ok(())
  }

  /// Move the cursor to the given position.
  fn goto(&self, x: u16, y: u16) -> Result<()> {
    let x = self.bbox.get().x + x + 1;
    let y = self.bbox.get().y + y + 1;
    write!(self.writer.borrow_mut(), "{}", Goto(x, y))
  }

  /// Clear the terminal content.
  fn clear_all(&self) -> Result<()> {
    write!(self.writer.borrow_mut(), "{}{}{}", Fg(Reset), Bg(Reset), All)
  }

  /// Flush everything written so far.
  fn flush(&self) -> Result<()> {
    self.writer.borrow_mut().flush()
  }

  /// Hide the cursor.
  fn hide(&self) -> Result<()> {
    write!(self.writer.borrow_mut(), "{}", Hide)
  }

  /// Show the cursor.
  fn show(&self) -> Result<()> {
    write!(self.writer.borrow_mut(), "{}", Show)
  }
}

pub struct TermRenderer<W>
where
  W: Write,
{
  writer: ClippingWriter<BufWriter<W>>,
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
    let writer = ClippingWriter::new(BufWriter::new(writer));

    Ok(TermRenderer {
      writer: writer,
    })
  }

  /// Retrieve the number of tasks that fit in the given `BBox`.
  fn displayable_tasks(&self, bbox: BBox) -> usize {
    ((bbox.h - MAIN_MARGIN_Y) / TASK_SPACE) as usize
  }

  /// Retrieve the number of tabs that fit in the given `BBox`.
  fn displayable_tabs(&self, bbox: BBox) -> usize {
    (bbox.w / TAB_TITLE_WIDTH) as usize
  }

  /// Render a `TermUi`.
  fn render_term_ui(&self, _ui: &TermUi, bbox: BBox) -> Result<BBox> {
    Ok(bbox)
  }

  /// Render a `TabBar`.
  fn render_tab_bar(&self, tab_bar: &TabBar, mut bbox: BBox) -> Result<BBox> {
    let mut x = 0;

    // TODO: We have some amount of duplication with the logic used to
    //       render a TaskListBox. Deduplicate?
    // TODO: This logic (and the task rendering, although for it the
    //       consequences are less severe) is not correct in the face of
    //       terminal resizes. If the width of the terminal is increased
    //       the offset would need to be adjusted. Should/can this be
    //       fixed?
    let limit = self.displayable_tabs(bbox);
    let selection = tab_bar.selection();
    let offset = sanitize_offset(tab_bar.offset(), selection, limit);

    for (i, tab) in tab_bar.iter().enumerate().skip(offset).take(limit) {
      let (fg, bg) = if i == selection {
        (SELECTED_QUERY_FG, SELECTED_QUERY_BG)
      } else {
        (UNSELECTED_QUERY_FG, UNSELECTED_QUERY_BG)
      };

      let title = align_center(tab.clone(), TAB_TITLE_WIDTH as usize - 4);
      let padded = format!("  {}  ", title);
      self.writer.write(x, 0, fg, bg, padded)?;

      x += TAB_TITLE_WIDTH;
    }

    if x < bbox.w {
      let pad = repeat(" ").take((bbox.w - x) as usize).collect::<String>();
      let fg = UNSELECTED_QUERY_FG;
      let bg = UNSELECTED_QUERY_BG;
      self.writer.write(x, 0, fg, bg, pad)?
    }

    tab_bar.reoffset(offset);

    // Account for the one line the tab bar occupies at the top and
    // another one to have some space at the bottom to the input/output
    // area.
    bbox.y += 1;
    bbox.h -= 2;
    Ok(bbox)
  }

  /// Render a `TaskListBox`.
  fn render_task_list_box(&self, task_list: &TaskListBox, bbox: BBox) -> Result<BBox> {
    let x = MAIN_MARGIN_X;
    let mut y = MAIN_MARGIN_Y;
    let mut cursor = None;

    let query = task_list.query();
    let limit = self.displayable_tasks(bbox);
    let selection = task_list.selection();
    let offset = sanitize_offset(task_list.offset(), selection, limit);

    query.enumerate::<Error, _>(|i, task| {
      if i >= offset + limit {
        Ok(false)
      } else if i < offset {
        Ok(true)
      } else {
        let complete = task.is_complete();
        let (state, state_fg, state_bg) = if !complete {
          ("[ ]", TASK_NOT_STARTED_FG, TASK_NOT_STARTED_BG)
        } else {
          ("[X]", TASK_DONE_FG, TASK_DONE_BG)
        };

        let (task_fg, task_bg) = if i == selection {
          (SELECTED_TASK_FG, &SELECTED_TASK_BG as &Color)
        } else {
          (UNSELECTED_TASK_FG, &UNSELECTED_TASK_BG as &Color)
        };

        self.writer.write(x, y, state_fg, state_bg, state)?;
        let x = x + state.len() as u16 + 1;
        self.writer.write(x, y, task_fg, task_bg, &task.summary)?;

        if i == selection {
          cursor = Some((x, y));
        }

        y += TASK_SPACE;
        Ok(true)
      }
    })?;

    // Set the cursor to the first character of the selected item. This
    // allows for more convenient copying of the currently selected task
    // with programs such as tmux.
    if let Some((x, y)) = cursor {
      self.writer.goto(x, y)?;
    }

    // We need to adjust the offset we use in order to be able to give
    // the correct impression of a sliding selection window.
    task_list.reoffset(offset);
    Ok(bbox)
  }

  /// Render an `InOutArea`.
  fn render_input_output(&self, in_out: &InOutArea, bbox: BBox, cap: &Cap) -> Result<BBox> {
    let (prefix, fg, bg, string) = match in_out.state() {
      InOut::Saved => (SAVED_TEXT, IN_OUT_SUCCESS_FG, IN_OUT_SUCCESS_BG, None),
      InOut::Error(ref e) => (ERROR_TEXT, IN_OUT_ERROR_FG, IN_OUT_ERROR_BG, Some(e)),
      InOut::Input(ref s, _) => (INPUT_TEXT, IN_OUT_SUCCESS_FG, IN_OUT_SUCCESS_BG, Some(s)),
      InOut::Clear => return Ok(Default::default()),
    };

    self.writer.write(0, bbox.h - 1, fg, bg, prefix)?;

    if let Some(string) = string {
      let x = prefix.len() as u16 + 1;
      let fg = IN_OUT_STRING_FG;
      let bg = IN_OUT_STRING_BG;

      self.writer.write(x, bbox.h - 1, fg, bg, string)?;

      if let InOut::Input(_, idx) = in_out.state() {
        debug_assert!(cap.is_focused(in_out.id()));
        self.writer.goto(x + *idx as u16, bbox.h - 1)?;
        self.writer.show()?
      }
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
    // By default we disable the cursor, but we may opt for enabling it
    // again when rendering certain widgets.
    let err = self.writer.clear_all().and_then(|_| self.writer.hide());

    if let Err(e) = err {
      panic!("Pre-render failed: {}", e);
    }
  }

  fn render(&self, widget: &Widget, bbox: BBox, cap: &Cap) -> BBox {
    let result;

    self.writer.restrict(bbox);

    if let Some(ui) = widget.downcast_ref::<TermUi>() {
      result = self.render_term_ui(ui, bbox)
    } else if let Some(in_out) = widget.downcast_ref::<InOutArea>() {
      result = self.render_input_output(in_out, bbox, cap);
    } else if let Some(tab_bar) = widget.downcast_ref::<TabBar>() {
      result = self.render_tab_bar(tab_bar, bbox);
    } else if let Some(task_list) = widget.downcast_ref::<TaskListBox>() {
      result = self.render_task_list_box(task_list, bbox);
    } else {
      panic!("Widget {:?} is unknown to the renderer", widget)
    }

    match result {
      Ok(b) => b,
      Err(e) => panic!("Rendering failed: {}", e),
    }
  }

  fn post_render(&self) {
    let err = self.writer.flush();
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
    let _ = self.writer.show();
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn pad_string() {
    assert_eq!(align_center("", 0), "");
    assert_eq!(align_center("", 1), " ");
    assert_eq!(align_center("", 8), "        ");
    assert_eq!(align_center("a", 1), "a");
    assert_eq!(align_center("a", 2), "a ");
    assert_eq!(align_center("a", 3), " a ");
    assert_eq!(align_center("a", 4), " a  ");
    assert_eq!(align_center("hello", 20), "       hello        ");
  }

  #[test]
  fn crop_string() {
    assert_eq!(align_center("hello", 4), "h...");
    assert_eq!(align_center("hello", 5), "hello");
    assert_eq!(align_center("that's a test", 8), "that'...");
  }

  #[test]
  fn clip_string() {
    let bbox = BBox {
      x: 0,
      y: 0,
      w: 0,
      h: 0,
    };
    assert_eq!(clip(0, 0, "hello", bbox), "");
    assert_eq!(clip(1, 0, "foobar", bbox), "");
    assert_eq!(clip(1, 1, "baz?", bbox), "");

    // The position of the bounding box should not matter.
    for x in 0..5 {
      for y in 0..3 {
        let bbox = BBox {
          x: x,
          y: y,
          w: 5,
          h: 3,
        };
        assert_eq!(clip(0, 2, "inside", bbox), "insid");
        assert_eq!(clip(0, 3, "outside", bbox), "");

        assert_eq!(clip(1, 2, "inside", bbox), "insi");
        assert_eq!(clip(2, 0, "inside", bbox), "ins");
        assert_eq!(clip(2, 3, "outside", bbox), "");
      }
    }
  }
}
