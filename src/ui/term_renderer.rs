// Copyright (C) 2018-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::Cell;
use std::cell::RefCell;
use std::cmp::max;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::io::BufWriter;
use std::io::Result;
use std::io::Write;

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

use super::dialog::Dialog;
use super::dialog::SetUnsetTag;
use super::in_out::InOut;
use super::in_out::InOutArea;
use super::tab_bar::TabBar;
use super::task_list_box::TaskListBox;
use super::termui::TermUi;

const TASK_LIST_MARGIN_X: u16 = 3;
const TASK_LIST_MARGIN_Y: u16 = 2;
const TASK_SPACE: u16 = 2;
const TAG_SPACE: u16 = 2;
const TAB_TITLE_WIDTH: u16 = 30;
const DIALOG_MARGIN_X: u16 = 2;
const DIALOG_MARGIN_Y: u16 = 1;
const DIALOG_MIN_W: u16 = 40;
const DIALOG_MIN_H: u16 = 20;

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

  match length.cmp(&width) {
    Ordering::Greater => {
      // Note: May underflow if width < 3. That's not really a supported
      //       use case, though, so we ignore it here.
      string.replace_range(width - 3..length, "...");
    },
    Ordering::Less => {
      let pad_right = (width - length) / 2;
      let pad_left = width - length - pad_right;

      let pad_right = " ".repeat(pad_right);
      let pad_left = " ".repeat(pad_left);

      string.insert_str(0, &pad_right);
      string.push_str(&pad_left);
    },
    Ordering::Equal => (),
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
    Self {
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
    write!(
      self.writer.borrow_mut(),
      "{}{}{}",
      Fg(Reset),
      Bg(Reset),
      All
    )
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


/// Retrieve the number of tasks that fit in the given `BBox`.
fn displayable_tasks(bbox: BBox) -> usize {
  ((bbox.h - TASK_LIST_MARGIN_Y) / TASK_SPACE) as usize
}

/// Retrieve the number of tags that fit in the given `BBox`.
fn displayable_tags(bbox: BBox) -> usize {
  ((bbox.h - 2 * DIALOG_MARGIN_Y) / TAG_SPACE) as usize
}

/// Retrieve the number of tabs that fit in the given `BBox`.
fn displayable_tabs(width: u16) -> usize {
  (width / TAB_TITLE_WIDTH) as usize
}


/// A renderer outputting to a terminal.
pub struct TermRenderer<W>
where
  W: Write,
{
  /// Our actual writer.
  writer: ClippingWriter<BufWriter<W>>,
  /// A mapping from widget ID to a widget-specific offset indicating
  /// where to start rendering.
  data: RefCell<HashMap<Id, OffsetData>>,
  /// The colors to use.
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
      writer,
      data: Default::default(),
      colors,
    })
  }

  /// Render a `TermUi`.
  fn render_term_ui(&self, _ui: &TermUi, bbox: BBox) -> Result<BBox> {
    Ok(bbox)
  }

  /// Render a `TabBar`.
  fn render_tab_bar(&self, tab_bar: &TabBar, cap: &dyn Cap, mut bbox: BBox) -> Result<BBox> {
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
    let count = tab_bar.iter(cap).len();
    let limit = displayable_tabs(w - 1);
    let selection = tab_bar.selection(cap);
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

    for (i, tab) in tab_bar.iter(cap).enumerate().skip(offset).take(limit) {
      let (fg, bg) = if i == selection {
        (self.colors.selected_query_fg, self.colors.selected_query_bg)
      } else {
        (
          self.colors.unselected_query_fg,
          self.colors.unselected_query_bg,
        )
      };

      let title = align_center(tab.clone(), TAB_TITLE_WIDTH as usize - 4);
      let padded = format!("  {}  ", title);
      self.writer.write(x, 0, fg, bg, padded)?;

      x += TAB_TITLE_WIDTH;
    }

    if x < w {
      let pad = " ".repeat((w - x) as usize);
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
  fn render_task_list_box(
    &self,
    task_list: &TaskListBox,
    cap: &dyn Cap,
    bbox: BBox,
  ) -> Result<BBox> {
    let mut map = self.data.borrow_mut();
    let data = map.entry(task_list.id()).or_default();

    let x = TASK_LIST_MARGIN_X;
    let mut y = TASK_LIST_MARGIN_Y;
    let mut cursor = None;

    let query = task_list.query(cap);
    let limit = displayable_tasks(bbox);
    let selection = task_list.selection(cap);
    let offset = sanitize_offset(data.offset, selection, limit);

    for (i, task) in query.iter().clone().enumerate().skip(offset).take(limit) {
      let complete = task.is_complete();
      let (state, state_fg, state_bg) = if !complete {
        (
          "[ ]",
          self.colors.task_not_started_fg,
          self.colors.task_not_started_bg,
        )
      } else {
        ("[X]", self.colors.task_done_fg, self.colors.task_done_bg)
      };

      let (task_fg, task_bg) = if i == selection {
        (self.colors.selected_task_fg, self.colors.selected_task_bg)
      } else {
        (
          self.colors.unselected_task_fg,
          self.colors.unselected_task_bg,
        )
      };

      self.writer.write(x, y, state_fg, state_bg, state)?;
      let x = x + state.len() as u16 + 1;
      self.writer.write(x, y, task_fg, task_bg, &task.summary)?;

      if i == selection && cap.is_focused(task_list.id()) {
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

  /// Fill a line of the dialog.
  fn fill_dialog_line(&self, x: u16, y: u16, w: u16) -> Result<()> {
    (x..w).try_for_each(|x| {
      self
        .writer
        .write(x, y, self.colors.dialog_fg, self.colors.dialog_bg, " ")
    })
  }

  /// Render a full line of the dialog, containing a tag.
  fn render_dialog_tag_line(
    &self,
    tag: &SetUnsetTag,
    y: u16,
    w: u16,
    selected: bool,
  ) -> Result<()> {
    let set = tag.is_set();
    let (state, state_fg, state_bg) = if set {
      (
        "[X]",
        self.colors.dialog_tag_set_fg,
        self.colors.dialog_tag_set_bg,
      )
    } else {
      (
        "[ ]",
        self.colors.dialog_tag_unset_fg,
        self.colors.dialog_tag_unset_bg,
      )
    };

    let (tag_fg, tag_bg) = if selected {
      (
        self.colors.dialog_selected_tag_fg,
        self.colors.dialog_selected_tag_bg,
      )
    } else {
      (self.colors.dialog_fg, self.colors.dialog_bg)
    };

    let mut x = 0;
    // Fill initial margin before state indication.
    self.fill_dialog_line(x, y, DIALOG_MARGIN_X)?;
    x += DIALOG_MARGIN_X;

    self.writer.write(x, y, state_fg, state_bg, state)?;
    x += state.len() as u16;

    self.fill_dialog_line(x, y, x + 1)?;
    x += 1;

    self.writer.write(x, y, tag_fg, tag_bg, tag.name())?;

    // Fill the remainder of the line.
    self.fill_dialog_line(x + tag.name().len() as u16, y, w)?;
    Ok(())
  }

  /// Render a `Dialog`.
  fn render_dialog(&self, dialog: &Dialog, cap: &dyn Cap, bbox: BBox) -> Result<BBox> {
    let mut map = self.data.borrow_mut();
    let data = map.entry(dialog.id()).or_default();

    let limit = displayable_tags(bbox);
    let selection = dialog.selection(cap);
    let offset = sanitize_offset(data.offset, selection, limit);

    let mut tags = dialog.tags(cap).iter().enumerate().skip(offset);

    (0..bbox.h).try_for_each(|y| {
      if y < DIALOG_MARGIN_Y
        || y >= bbox.h - DIALOG_MARGIN_Y
        || (y - DIALOG_MARGIN_Y) % TAG_SPACE != 0
      {
        self.fill_dialog_line(0, y, bbox.w)
      } else if let Some((i, tag)) = tags.next() {
        self.render_dialog_tag_line(tag, y, bbox.w, i == selection)
      } else {
        self.fill_dialog_line(0, y, bbox.w)
      }
    })?;

    if cap.is_focused(dialog.id()) {
      let x = DIALOG_MARGIN_X + 4;
      let y = DIALOG_MARGIN_Y + (selection as u16 * TAG_SPACE);
      self.writer.goto(x, y)?;
    }

    data.offset = offset;
    Ok(bbox)
  }

  /// Render an `InOutArea`.
  fn render_input_output(&self, in_out: &InOutArea, cap: &dyn Cap, bbox: BBox) -> Result<BBox> {
    let (prefix, fg, bg, string) = match in_out.state(cap) {
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

      if let InOut::Input(s, idx) = in_out.state(cap) {
        debug_assert!(cap.is_focused(in_out.id()));

        let idx = char_index(s, *idx);
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
      Ok((w, h)) => BBox { x: 0, y: 0, w, h },
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

  fn render(&self, widget: &dyn Renderable, cap: &dyn Cap, bbox: BBox) -> BBox {
    self.writer.restrict(bbox);

    let result = if let Some(ui) = widget.downcast_ref::<TermUi>() {
      self.render_term_ui(ui, bbox)
    } else if let Some(dialog) = widget.downcast_ref::<Dialog>() {
      // We want the dialog box displayed in the center and not filling
      // up the entire screen.
      let w = max(DIALOG_MIN_W, bbox.w / 2);
      let h = max(DIALOG_MIN_H, bbox.h / 2);
      let x = w / 2;
      let y = h / 2;

      let bbox = BBox { x, y, w, h };
      self.writer.restrict(bbox);

      self.render_dialog(dialog, cap, bbox)
    } else if let Some(in_out) = widget.downcast_ref::<InOutArea>() {
      self.render_input_output(in_out, cap, bbox)
    } else if let Some(tab_bar) = widget.downcast_ref::<TabBar>() {
      self.render_tab_bar(tab_bar, cap, bbox)
    } else if let Some(task_list) = widget.downcast_ref::<TaskListBox>() {
      self.render_task_list_box(task_list, cap, bbox)
    } else {
      panic!("Widget {:?} is unknown to the renderer", widget)
    };

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
    // We should never panic in a destructor so don't unwrap and just
    // swallow the result. We are done anyway.
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

    let bbox = BBox {
      x: 10,
      y: 16,
      w: 6,
      h: 1,
    };
    assert_eq!(clip(0, 0, "hello", bbox), "hello");
    assert_eq!(clip(0, 0, "hello you", bbox), "hello ");

    // The position of the bounding box should not matter.
    for x in 0..5 {
      for y in 0..3 {
        let bbox = BBox { x, y, w: 5, h: 3 };
        assert_eq!(clip(0, 2, "inside", bbox), "insid");
        assert_eq!(clip(0, 3, "outside", bbox), "");

        assert_eq!(clip(1, 2, "inside", bbox), "insi");
        assert_eq!(clip(2, 0, "inside", bbox), "ins");
        assert_eq!(clip(2, 3, "outside", bbox), "");
      }
    }
  }
}
