// term_renderer.rs

// *************************************************************************
// * Copyright (C) 2018-2020 Daniel Mueller (deso@posteo.net)              *
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
use std::collections::HashMap;
use std::io::BufWriter;
use std::io::Result;
use std::io::Write;
use std::iter::repeat;

use termion::clear::All;
use termion::color::Bg;
use termion::color::Fg;
use termion::color::Reset;
use termion::cursor::Goto;
use termion::cursor::Hide;
use termion::cursor::Show;
use termion::terminal_size;

use gui::BBox;
use gui::Cap;
use gui::Id;
use gui::Object;
use gui::Renderable;
use gui::Renderer;

use crate::colors::Color;
use crate::colors::Colors;

use super::in_out::InOut;
use super::in_out::InOutArea;
use super::tab_bar::TabBar;
use super::task_list_box::TaskListBox;
use super::termui::TermUi;

const MAIN_MARGIN_X: u16 = 3;
const MAIN_MARGIN_Y: u16 = 2;
const TASK_SPACE: u16 = 2;
const TAB_TITLE_WIDTH: u16 = 30;

const SAVED_TEXT: &str = " Saved ";
const SEARCH_TEXT: &str = " Search ";
const ERROR_TEXT: &str = " Error ";
const INPUT_TEXT: &str = " > ";


/// Find the character index that maps to the given byte position.
fn char_index(s: &str, pos: usize) -> usize {
  let mut count = 0;
  for (idx, c) in s.char_indices() {
    if pos < idx + c.len_utf8() {
      break
    }
    count += 1;
  }
  count
}

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
  fn write<S>(&self, x: u16, y: u16, fg: Color, bg: Color, string: S) -> Result<()>
  where
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
        Fg(fg.as_term_color()),
        Bg(bg.as_term_color()),
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


/// A struct containing rendering related widget data with an offset.
#[derive(Default)]
struct OffsetData {
  pub offset: usize,
}


pub struct TermRenderer<W>
where
  W: Write,
{
  writer: ClippingWriter<BufWriter<W>>,
  data: RefCell<HashMap<Id, OffsetData>>,
  colors: Colors,
}

impl<W> TermRenderer<W>
where
  W: Write,
{
  /// Create a new `TermRenderer` object.
  pub fn new(writer: W, colors: Colors) -> Result<Self> {
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
      data: Default::default(),
      colors,
    })
  }

  /// Retrieve the number of tasks that fit in the given `BBox`.
  fn displayable_tasks(&self, bbox: BBox) -> usize {
    ((bbox.h - MAIN_MARGIN_Y) / TASK_SPACE) as usize
  }

  /// Retrieve the number of tabs that fit in the given `BBox`.
  fn displayable_tabs(&self, width: u16) -> usize {
    (width / TAB_TITLE_WIDTH) as usize
  }

  /// Render a `TermUi`.
  fn render_term_ui(&self, _ui: &TermUi, bbox: BBox) -> Result<BBox> {
    Ok(bbox)
  }

  /// Render a `TabBar`.
  fn render_tab_bar(&self, tab_bar: &TabBar, mut bbox: BBox) -> Result<BBox> {
    let mut map = self.data.borrow_mut();
    let data = map.entry(tab_bar.id()).or_default();

    let mut x = 1;
    let w = bbox.w - 1;

    // TODO: We have some amount of duplication with the logic used to
    //       render a TaskListBox. Deduplicate?
    // TODO: This logic (and the task rendering, although for it the
    //       consequences are less severe) is not correct in the face of
    //       terminal resizes. If the width of the terminal is increased
    //       the offset would need to be adjusted. Should/can this be
    //       fixed?
    let count = tab_bar.iter().len();
    let limit = self.displayable_tabs(w - 1);
    let selection = tab_bar.selection();
    let offset = sanitize_offset(data.offset, selection, limit);

    if offset > 0 {
      let fg = self.colors.more_tasks_fg;
      let bg = self.colors.more_tasks_bg;
      self.writer.write(0, 0, fg, bg, "<")?;
    } else {
      let fg = self.colors.unselected_query_fg;
      let bg = self.colors.unselected_query_bg;
      self.writer.write(0, 0, fg, bg, " ")?;
    }

    if count > offset + limit {
      let fg = self.colors.more_tasks_fg;
      let bg = self.colors.more_tasks_bg;
      self.writer.write(bbox.w - 1, 0, fg, bg, ">")?;
    } else {
      let fg = self.colors.unselected_query_fg;
      let bg = self.colors.unselected_query_bg;
      self.writer.write(bbox.w - 1, 0, fg, bg, " ")?;
    }

    for (i, tab) in tab_bar.iter().enumerate().skip(offset).take(limit) {
      let (fg, bg) = if i == selection {
        (self.colors.selected_query_fg, self.colors.selected_query_bg)
      } else {
        (self.colors.unselected_query_fg, self.colors.unselected_query_bg)
      };

      let title = align_center(tab.clone(), TAB_TITLE_WIDTH as usize - 4);
      let padded = format!("  {}  ", title);
      self.writer.write(x, 0, fg, bg, padded)?;

      x += TAB_TITLE_WIDTH;
    }

    if x < w {
      let pad = repeat(" ").take((w - x) as usize).collect::<String>();
      let fg = self.colors.unselected_query_fg;
      let bg = self.colors.unselected_query_bg;
      self.writer.write(x, 0, fg, bg, pad)?
    }

    data.offset = offset;

    // Account for the one line the tab bar occupies at the top and
    // another one to have some space at the bottom to the input/output
    // area.
    bbox.y += 1;
    bbox.h -= 2;
    Ok(bbox)
  }

  /// Render a `TaskListBox`.
  fn render_task_list_box(&self, task_list: &TaskListBox, bbox: BBox) -> Result<BBox> {
    let mut map = self.data.borrow_mut();
    let data = map.entry(task_list.id()).or_default();

    let x = MAIN_MARGIN_X;
    let mut y = MAIN_MARGIN_Y;
    let mut cursor = None;

    let query = task_list.query();
    let limit = self.displayable_tasks(bbox);
    let selection = task_list.selection();
    let offset = sanitize_offset(data.offset, selection, limit);

    for (i, task) in query.iter().clone().enumerate().skip(offset).take(limit) {
      let complete = task.is_complete();
      let (state, state_fg, state_bg) = if !complete {
        ("[ ]", self.colors.task_not_started_fg, self.colors.task_not_started_bg)
      } else {
        ("[X]", self.colors.task_done_fg, self.colors.task_done_bg)
      };

      let (task_fg, task_bg) = if i == selection {
        (self.colors.selected_task_fg, self.colors.selected_task_bg)
      } else {
        (self.colors.unselected_task_fg, self.colors.unselected_task_bg)
      };

      self.writer.write(x, y, state_fg, state_bg, state)?;
      let x = x + state.len() as u16 + 1;
      self.writer.write(x, y, task_fg, task_bg, &task.summary)?;

      if i == selection {
        cursor = Some((x, y));
      }

      y += TASK_SPACE;
    }

    // Set the cursor to the first character of the selected item. This
    // allows for more convenient copying of the currently selected task
    // with programs such as tmux.
    if let Some((x, y)) = cursor {
      self.writer.goto(x, y)?;
    }

    // We need to adjust the offset we use in order to be able to give
    // the correct impression of a sliding selection window.
    data.offset = offset;
    Ok(bbox)
  }

  /// Render an `InOutArea`.
  fn render_input_output(&self, in_out: &InOutArea, bbox: BBox, cap: &dyn Cap) -> Result<BBox> {
    let (prefix, fg, bg, string) = match in_out.state() {
      InOut::Saved => (
        SAVED_TEXT,
        self.colors.in_out_success_fg,
        self.colors.in_out_success_bg,
        None,
      ),
      InOut::Search(ref s) => (
        SEARCH_TEXT,
        self.colors.in_out_status_fg,
        self.colors.in_out_status_bg,
        Some(s),
      ),
      InOut::Error(ref e) => (
        ERROR_TEXT,
        self.colors.in_out_error_fg,
        self.colors.in_out_error_bg,
        Some(e),
      ),
      InOut::Input(ref s, _) => (
        INPUT_TEXT,
        self.colors.in_out_success_fg,
        self.colors.in_out_success_bg,
        Some(s),
      ),
      InOut::Clear => return Ok(Default::default()),
    };

    self.writer.write(0, bbox.h - 1, fg, bg, prefix)?;

    if let Some(string) = string {
      let x = prefix.len() as u16 + 1;
      let fg = self.colors.in_out_string_fg;
      let bg = self.colors.in_out_string_bg;

      self.writer.write(x, bbox.h - 1, fg, bg, string)?;

      if let InOut::Input(s, idx) = in_out.state() {
        debug_assert!(cap.is_focused(in_out.id()));

        let idx = char_index(&s, *idx);
        self.writer.goto(x + idx as u16, bbox.h - 1)?;
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

  fn render(&self, widget: &dyn Renderable, bbox: BBox, cap: &dyn Cap) -> BBox {
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
