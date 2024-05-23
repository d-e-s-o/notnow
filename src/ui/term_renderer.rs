// Copyright (C) 2018-2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::Cell;
use std::cell::RefCell;
use std::cmp::max;
use std::cmp::min;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::io::BufWriter;
use std::io::Result;
use std::io::Write;
use std::ops::Add;
use std::ops::Sub;

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
use gui::Widget as _;

use crate::colors::Color;
use crate::colors::Colors;
use crate::tasks::Task;
use crate::text;
use crate::text::Cursor;
use crate::text::DisplayWidth as _;
use crate::text::Width;
use crate::LINE_END;

use super::detail_dialog::DetailDialog;
use super::detail_dialog::DetailDialogData;
use super::in_out::InOut;
use super::in_out::InOutArea;
use super::tab_bar::TabBar;
use super::tag_dialog::SetUnsetTag;
use super::tag_dialog::TagDialog;
use super::task_list_box::TaskListBox;
use super::termui::TermUi;

const TASK_LIST_MARGIN_X: u16 = 3;
const TASK_LIST_MARGIN_Y: u16 = 2;
const TASK_SPACE: u16 = 2;
const TAG_SPACE: u16 = 2;
const TAB_TITLE_WIDTH: u16 = 30;
const DETAIL_DIALOG_MIN_W: u16 = 40;
const DETAIL_DIALOG_MIN_H: u16 = 20;
const DETAIL_DIALOG_MARGIN_X: u16 = 2;
const DETAIL_DIALOG_MARGIN_Y: u16 = 1;
const TAG_DIALOG_MARGIN_X: u16 = 2;
const TAG_DIALOG_MARGIN_Y: u16 = 1;
const TAG_DIALOG_MIN_W: u16 = 40;
const TAG_DIALOG_MIN_H: u16 = 20;

const SAVED_TEXT: &str = " Saved ";
const SEARCH_TEXT: &str = " Search ";
const ERROR_TEXT: &str = " Error ";
const INPUT_TEXT: &str = " > ";


/// Calculate the desired starting position of a window, given a cursor
/// that is always meant to be covered by `<return value> + size`.
///
/// This functionality forms the basis for all our sliding windows over
/// a set of entities (such as tasks, tags, ...).
fn window_start<T, U>(start: T, size: U, cursor: T) -> T
where
  T: Copy + Ord + Add<U, Output = T> + Sub<U, Output = T>,
  U: Copy + From<usize>,
{
  // If the cursor is in front of the window it marks the start of the
  // window. If it is past the end of the window, the window's start is
  // adjusted such that the cursor ends up being the very last element
  // in it.
  if cursor <= start {
    cursor
  } else if cursor >= start + size {
    cursor - size + U::from(1)
  } else {
    start
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
    text::clip(string, Width::from(usize::from(w - x)))
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

  /// Fill the rest of a line with the given color.
  fn fill_line(&self, x: u16, y: u16, w: u16, color: Color) -> Result<()> {
    // A string of 200 spaces.
    static SPACES: &str = concat!(
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
      "          ",
    );

    let w = min(self.bbox.get().w.saturating_sub(x), w);
    let x = self.bbox.get().x + x + 1;
    let y = self.bbox.get().y + y + 1;

    let mut x = x;
    let mut w = w;
    while w > 0 {
      let cells = min(SPACES.len(), w.into());
      // SANITY: We will always get a valid slice here, because `cells`
      //         is guaranteed to be less `SPACES.len()` and more than
      //         1.
      let s = SPACES.get(..cells).unwrap();
      let () = write!(
        self.writer.borrow_mut(),
        "{}{}{}{}",
        Goto(x, y),
        Fg(color.as_term_color()),
        Bg(color.as_term_color()),
        s,
      )?;

      x += cells as u16;
      w -= cells as u16;
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
  ((bbox.h.saturating_sub(TASK_LIST_MARGIN_Y)) / TASK_SPACE) as usize
}

/// Retrieve the number of tags that fit in the given `BBox`.
fn displayable_tags(bbox: BBox) -> usize {
  ((bbox.h.saturating_sub(2 * TAG_DIALOG_MARGIN_Y)) / TAG_SPACE) as usize
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
    let w = bbox.w.saturating_sub(1);

    // TODO: We have some amount of duplication with the logic used to
    //       render a TaskListBox. Deduplicate?
    // TODO: This logic (and the task rendering, although for it the
    //       consequences are less severe) is not correct in the face of
    //       terminal resizes. If the width of the terminal is increased
    //       the offset would need to be adjusted. Should/can this be
    //       fixed?
    let tabs = tab_bar.iter(cap).len();
    let count = displayable_tabs(w.saturating_sub(1));
    let selection = tab_bar.selection(cap);
    let offset = window_start(data.offset, count, selection);

    if offset > 0 {
      let fg = self.colors.more_tasks_fg;
      let bg = self.colors.more_tasks_bg;
      self.writer.write(0, 0, fg, bg, "<")?;
    } else {
      let fg = self.colors.unselected_tab_fg;
      let bg = self.colors.unselected_tab_bg;
      self.writer.write(0, 0, fg, bg, " ")?;
    }

    if tabs > offset + count {
      let fg = self.colors.more_tasks_fg;
      let bg = self.colors.more_tasks_bg;
      self.writer.write(w, 0, fg, bg, ">")?;
    } else {
      let fg = self.colors.unselected_tab_fg;
      let bg = self.colors.unselected_tab_bg;
      self.writer.write(w, 0, fg, bg, " ")?;
    }

    for (i, tab) in tab_bar.iter(cap).enumerate().skip(offset).take(count) {
      let (fg, bg) = if i == selection {
        (self.colors.selected_tab_fg, self.colors.selected_tab_bg)
      } else {
        (self.colors.unselected_tab_fg, self.colors.unselected_tab_bg)
      };

      let title = align_center(tab.clone(), TAB_TITLE_WIDTH as usize - 4);
      let padded = format!("  {}  ", title);
      self.writer.write(x, 0, fg, bg, padded)?;

      x += TAB_TITLE_WIDTH;
    }

    if x < w {
      let pad = " ".repeat((w - x) as usize);
      let fg = self.colors.unselected_tab_fg;
      let bg = self.colors.unselected_tab_bg;
      self.writer.write(x, 0, fg, bg, pad)?
    }

    data.offset = offset;

    // Account for the one line the tab bar occupies at the top and
    // another one to have some space at the bottom to the input/output
    // area.
    bbox.y = bbox.y.saturating_add(1);
    bbox.h = bbox.h.saturating_sub(2);
    Ok(bbox)
  }

  /// Render a full line of the [`TaskListBox`], containing a task.
  fn render_task_list_line(
    &self,
    task: &Task,
    tagged: bool,
    selected: bool,
    y: u16,
    w: u16,
  ) -> Result<()> {
    let (state, state_fg, state_bg) = if !tagged {
      (
        "[ ]",
        self.colors.task_not_started_fg,
        self.colors.task_not_started_bg,
      )
    } else {
      ("[X]", self.colors.task_done_fg, self.colors.task_done_bg)
    };

    let (task_fg, task_bg) = if selected {
      (self.colors.selected_task_fg, self.colors.selected_task_bg)
    } else {
      (
        self.colors.unselected_task_fg,
        self.colors.unselected_task_bg,
      )
    };

    let mut x = 0;
    let () = self
      .writer
      .fill_line(0, y, TASK_LIST_MARGIN_X, self.colors.unselected_task_bg)?;
    x += TASK_LIST_MARGIN_X;

    self.writer.write(x, y, state_fg, state_bg, state)?;
    x += state.len() as u16;

    let details = if task.details().is_empty() {
      "   "
    } else {
      " * "
    };
    self.writer.write(
      x,
      y,
      self.colors.unselected_task_fg,
      self.colors.unselected_task_bg,
      details,
    )?;

    x += details.len() as u16;
    self.writer.write(x, y, task_fg, task_bg, task.summary())?;

    x += task.summary().display_width().as_usize() as u16;
    let () = self
      .writer
      .fill_line(x, y, w, self.colors.unselected_task_bg)?;
    Ok(())
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

    let mut cursor = None;

    let view = task_list.view(cap);
    let count = displayable_tasks(bbox);
    let selection = task_list.selection(cap);
    let offset = window_start(data.offset, count, selection);

    let () = view.iter(|iter| {
      let mut tasks = iter.enumerate().skip(offset).take(count);

      (0..bbox.h).try_for_each(|y| {
        if y < TASK_LIST_MARGIN_Y
          || y > bbox.h - TASK_LIST_MARGIN_Y
          || (y - TASK_LIST_MARGIN_Y) % TAG_SPACE != 0
        {
          self
            .writer
            .fill_line(0, y, bbox.w, self.colors.unselected_task_bg)
        } else if let Some((i, task)) = tasks.next() {
          let tagged = task_list
            .toggle_tag(cap)
            .map(|toggle_tag| task.has_tag(&toggle_tag))
            .unwrap_or(false);

          let () = self.render_task_list_line(task, tagged, i == selection, y, bbox.w)?;

          if i == selection && cap.is_focused(task_list.id()) {
            cursor = Some((TASK_LIST_MARGIN_X + 3, y));
          }
          Ok(())
        } else {
          self
            .writer
            .fill_line(0, y, bbox.w, self.colors.unselected_task_bg)
        }
      })
    })?;

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

  /// Render a full line of the dialog, containing a tag.
  fn render_detail_dialog_line<'s>(
    &self,
    line: Option<&'s str>,
    y: u16,
    w: u16,
  ) -> Result<(Width, Option<&'s str>)> {
    let fg = self.colors.detail_dialog_fg;
    let bg = self.colors.detail_dialog_bg;

    let mut x = 0;
    // Fill initial margin before content.
    let () = self
      .writer
      .fill_line(x, y, DETAIL_DIALOG_MARGIN_X, self.colors.detail_dialog_bg)?;
    x += DETAIL_DIALOG_MARGIN_X;

    let (width, rest) = if let Some(line) = line {
      let (line, rest) = text::wrap(
        line,
        Width::from(usize::from(w.saturating_sub(2 * DETAIL_DIALOG_MARGIN_X))),
      );
      let width = line.display_width();
      let () = self.writer.write(x, y, fg, bg, line)?;
      (width, rest)
    } else {
      (Width::from(0), None)
    };

    x += width.as_usize() as u16;

    // Fill the remainder of the line.
    let () = self
      .writer
      .fill_line(x, y, w, self.colors.detail_dialog_bg)?;
    Ok((width, rest))
  }

  /// Render a `DetailDialog`.
  fn render_detail_dialog(
    &self,
    detail_dialog: &DetailDialog,
    cap: &dyn Cap,
    bbox: BBox,
  ) -> Result<BBox> {
    let data = detail_dialog.data::<DetailDialogData>(cap);
    let details = data.details();
    let mut lines = details.as_str().split(LINE_END);
    let mut line = None;
    let mut selection = details.cursor();
    let mut cursor = None;

    (0..bbox.h).try_for_each(|y| {
      if y < DETAIL_DIALOG_MARGIN_Y || y >= bbox.h - DETAIL_DIALOG_MARGIN_Y {
        self
          .writer
          .fill_line(0, y, bbox.w, self.colors.detail_dialog_bg)
      } else {
        let (rendered, rest) =
          self.render_detail_dialog_line(line.or_else(|| lines.next()), y, bbox.w)?;

        if cursor.is_none() {
          if Cursor::at_start() + rendered >= selection {
            cursor = Some((
              selection + Width::from(usize::from(DETAIL_DIALOG_MARGIN_X)),
              y,
            ));
          } else {
            selection -= rendered;
          }

          if rest.is_none() {
            // If there is no rest then we need to account for the line
            // break that we split at.
            selection -= LINE_END.display_width();
          }
        }

        line = rest;
        Ok(())
      }
    })?;

    if let Some((x, y)) = cursor {
      let () = self.writer.goto(x.as_usize() as _, y)?;
      let () = self.writer.show()?;
    } else if cfg!(debug_assertions) {
      panic!("no cursor set")
    }
    Ok(bbox)
  }

  /// Render a full line of the dialog, containing a tag.
  fn render_tag_dialog_tag_line(
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
        self.colors.tag_dialog_tag_set_fg,
        self.colors.tag_dialog_tag_set_bg,
      )
    } else {
      (
        "[ ]",
        self.colors.tag_dialog_tag_unset_fg,
        self.colors.tag_dialog_tag_unset_bg,
      )
    };

    let (tag_fg, tag_bg) = if selected {
      (
        self.colors.tag_dialog_selected_tag_fg,
        self.colors.tag_dialog_selected_tag_bg,
      )
    } else {
      (self.colors.tag_dialog_fg, self.colors.tag_dialog_bg)
    };

    let mut x = 0;
    // Fill initial margin before state indication.
    let () = self
      .writer
      .fill_line(x, y, TAG_DIALOG_MARGIN_X, self.colors.tag_dialog_bg)?;
    x += TAG_DIALOG_MARGIN_X;

    let () = self.writer.write(x, y, state_fg, state_bg, state)?;
    x += state.len() as u16;

    let () = self.writer.fill_line(x, y, 1, self.colors.tag_dialog_bg)?;
    x += 1;

    let () = self.writer.write(x, y, tag_fg, tag_bg, tag.name())?;

    // Fill the remainder of the line.
    let () = self
      .writer
      .fill_line(x + tag.name().len() as u16, y, w, self.colors.tag_dialog_bg)?;
    Ok(())
  }

  /// Render a `TagDialog`.
  fn render_tag_dialog(&self, tag_dialog: &TagDialog, cap: &dyn Cap, bbox: BBox) -> Result<BBox> {
    let mut map = self.data.borrow_mut();
    let data = map.entry(tag_dialog.id()).or_default();

    let count = displayable_tags(bbox);
    let selection = tag_dialog.selection(cap);
    let offset = window_start(data.offset, count, selection);

    let mut tags = tag_dialog.tags(cap).iter().enumerate().skip(offset);

    (0..bbox.h).try_for_each(|y| {
      if y < TAG_DIALOG_MARGIN_Y
        || y >= bbox.h - TAG_DIALOG_MARGIN_Y
        || (y - TAG_DIALOG_MARGIN_Y) % TAG_SPACE != 0
      {
        self
          .writer
          .fill_line(0, y, bbox.w, self.colors.tag_dialog_bg)
      } else if let Some((i, tag)) = tags.next() {
        self.render_tag_dialog_tag_line(tag, y, bbox.w, i == selection)
      } else {
        self
          .writer
          .fill_line(0, y, bbox.w, self.colors.tag_dialog_bg)
      }
    })?;

    if cap.is_focused(tag_dialog.id()) {
      let x = TAG_DIALOG_MARGIN_X + 4;
      let y = TAG_DIALOG_MARGIN_Y + (selection as u16 * TAG_SPACE);
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
        Some(s.as_ref()),
      ),
      InOut::Error(ref e) => (
        ERROR_TEXT,
        self.colors.in_out_error_fg,
        self.colors.in_out_error_bg,
        Some(e.as_ref()),
      ),
      InOut::Input(ref text) => (
        INPUT_TEXT,
        self.colors.in_out_success_fg,
        self.colors.in_out_success_bg,
        Some(text.as_str()),
      ),
      InOut::Clear => {
        // This is a tiny bit of an unclean solution, but essentially we
        // do not want to keep any offset data around between editing
        // cycles. If we apply an offset previously stored for an overly
        // long task and then start editing a different task that easily
        // fits on the screen, the result will look strange. The correct
        // thing is to clear any offset data, but ideally there would be
        // a cleaner way to go about that than relying on us entering
        // the `Clear` state in between editing of different tasks.
        let _ = self.data.borrow_mut().remove(&in_out.id());
        return Ok(Default::default())
      },
    };

    self.writer.write(0, bbox.h - 1, fg, bg, prefix)?;

    if let Some(string) = string {
      let x = prefix.len() as u16 + 1;
      let fg = self.colors.in_out_string_fg;
      let bg = self.colors.in_out_string_bg;

      if let InOut::Input(text) = in_out.state(cap) {
        debug_assert!(cap.is_focused(in_out.id()));

        let mut map = self.data.borrow_mut();
        let data = map.entry(in_out.id()).or_default();

        // Calculate the number of displayable characters we have
        // available after the "prefix".
        let count = Width::from(bbox.w.saturating_sub(x) as usize);
        let cursor = text.cursor();
        let offset = window_start(
          text.cursor_start() + Width::from(data.offset),
          count,
          cursor,
        );
        let string = text.substr(offset..);

        data.offset = offset.as_usize();

        let () = self.writer.write(x, bbox.h - 1, fg, bg, string)?;
        let () = self.writer.goto(
          x + cursor.as_usize() as u16 - offset.as_usize() as u16,
          bbox.h - 1,
        )?;
        let () = self.writer.show()?;
      } else {
        let () = self.writer.write(x, bbox.h - 1, fg, bg, string)?;
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
      Err(err) => panic!("Retrieving terminal size failed: {err}"),
    }
  }

  fn pre_render(&self) {
    // By default we disable the cursor, but we may opt for enabling it
    // again when rendering certain widgets.
    let result = self.writer.clear_all().and_then(|_| self.writer.hide());

    if let Err(err) = result {
      panic!("Pre-render failed: {err}");
    }
  }

  fn render(&self, widget: &dyn Renderable, cap: &dyn Cap, bbox: BBox) -> BBox {
    self.writer.restrict(bbox);

    let result = if let Some(ui) = widget.downcast_ref::<TermUi>() {
      self.render_term_ui(ui, bbox)
    } else if let Some(detail_dialog) = widget.downcast_ref::<DetailDialog>() {
      // We want the dialog box displayed in the center and not filling
      // up the entire screen.
      let w = max(DETAIL_DIALOG_MIN_W, bbox.w / 2);
      let h = max(DETAIL_DIALOG_MIN_H, bbox.h / 2);
      let x = w / 2;
      let y = h / 2;

      let bbox = BBox { x, y, w, h };
      self.writer.restrict(bbox);

      self.render_detail_dialog(detail_dialog, cap, bbox)
    } else if let Some(tag_dialog) = widget.downcast_ref::<TagDialog>() {
      // We want the dialog box displayed in the center and not filling
      // up the entire screen.
      let w = max(TAG_DIALOG_MIN_W, bbox.w / 2);
      let h = max(TAG_DIALOG_MIN_H, bbox.h / 2);
      let x = w / 2;
      let y = h / 2;

      let bbox = BBox { x, y, w, h };
      self.writer.restrict(bbox);

      self.render_tag_dialog(tag_dialog, cap, bbox)
    } else if let Some(in_out) = widget.downcast_ref::<InOutArea>() {
      self.render_input_output(in_out, cap, bbox)
    } else if let Some(tab_bar) = widget.downcast_ref::<TabBar>() {
      self.render_tab_bar(tab_bar, cap, bbox)
    } else if let Some(task_list) = widget.downcast_ref::<TaskListBox>() {
      self.render_task_list_box(task_list, cap, bbox)
    } else {
      panic!("Widget {widget:?} is unknown to the renderer")
    };

    match result {
      Ok(b) => b,
      Err(err) => panic!("Rendering failed: {err}"),
    }
  }

  fn post_render(&self) {
    let result = self.writer.flush();
    if let Err(err) = result {
      panic!("Post-render failed: {err}");
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

  #[cfg(feature = "nightly")]
  use unstable_test::Bencher;


  /// Check that we can centrally align a string properly using
  /// `align_center`.
  #[test]
  fn align_string() {
    assert_eq!(align_center("", 0), "");
    assert_eq!(align_center("", 1), " ");
    assert_eq!(align_center("", 8), "        ");
    assert_eq!(align_center("a", 1), "a");
    assert_eq!(align_center("a", 2), "a ");
    assert_eq!(align_center("a", 3), " a ");
    assert_eq!(align_center("a", 4), " a  ");
    assert_eq!(align_center("hello", 20), "       hello        ");
  }

  /// Make sure that `align_center` crops a string as necessary.
  #[test]
  fn crop_string() {
    assert_eq!(align_center("hello", 4), "h...");
    assert_eq!(align_center("hello", 5), "hello");
    assert_eq!(align_center("that's a test", 8), "that'...");
  }

  /// Check that we ca properly clip a string using `clip`.
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

  /// Benchmark the rendering of the UI.
  // TODO: This is quite a monstrosity with all the setup code and there
  //       is duplication/overlap with respect to the example and other
  //       code paths. We should really streamline that.
  #[cfg(feature = "nightly")]
  #[ignore = "test requires fully functional TTY"]
  #[bench]
  fn bench_ui_rendering(b: &mut Bencher) {
    use std::ffi::OsString;
    use std::io::stdout;

    use gui::Ui;
    use tempfile::TempDir;
    use termion::raw::IntoRawMode as _;
    use termion::screen::IntoAlternateScreen as _;
    use tokio::runtime::Builder;

    use crate::test::default_tasks_and_tags;
    use crate::ui::Renderer as TermUiRenderer;
    use crate::ui::UiData as TermUiData;
    use crate::DirCap;
    use crate::TaskState;
    use crate::UiConfig;
    use crate::UiState;

    let rt = Builder::new_current_thread().build().unwrap();

    let () = rt.block_on(async {
      let (ui_config, task_state) = default_tasks_and_tags();

      let task_state = TaskState::with_serde(task_state).unwrap();
      let tasks_dir = TempDir::new().unwrap();
      let mut tasks_root_cap = DirCap::for_dir(tasks_dir.path().to_path_buf())
        .await
        .unwrap();
      let () = task_state.save(&mut tasks_root_cap).await.unwrap();

      let ui_config = UiConfig::with_serde(ui_config, &task_state).unwrap();
      let ui_config_dir = TempDir::new().unwrap();
      let ui_config_file_name = OsString::from("notnow.json");
      let ui_config_path = (
        ui_config_dir.path().to_path_buf(),
        ui_config_file_name.clone(),
      );
      let mut ui_config_dir_cap = DirCap::for_dir(ui_config_dir.path().to_path_buf())
        .await
        .unwrap();
      let ui_config_dir_write_guard = ui_config_dir_cap.write().await.unwrap();
      let mut ui_config_file_cap = ui_config_dir_write_guard.file_cap(&ui_config_file_name);
      let () = ui_config.save(&mut ui_config_file_cap).await.unwrap();

      let ui_state_dir = TempDir::new().unwrap();
      let ui_state_file_name = OsString::from("ui-state.json");
      let ui_state_path = (ui_state_dir.path().to_path_buf(), ui_state_file_name);
      let tasks_root = tasks_dir.path().to_path_buf();

      let task_state = TaskState::load(&tasks_root).await.unwrap();
      let ui_config_file = ui_config_path.0.join(&ui_config_path.1);
      let ui_state_file = ui_state_path.0.join(&ui_state_path.1);
      let ui_config = UiConfig::load(&ui_config_file, &task_state).await.unwrap();
      let UiConfig {
        colors,
        toggle_tag,
        views,
      } = ui_config;

      let ui_state = UiState::load(&ui_state_file).await.unwrap();

      let ui_config_dir_cap = DirCap::for_dir(ui_config_path.0).await.unwrap();
      let ui_config_file = ui_config_path.1;

      let ui_state_dir_cap = DirCap::for_dir(ui_state_path.0).await.unwrap();
      let ui_state_file = ui_state_path.1;

      let tasks_root_cap = DirCap::for_dir(tasks_root).await.unwrap();

      let (ui, _) = Ui::new(
        || {
          Box::new(TermUiData::new(
            tasks_root_cap,
            task_state,
            (ui_config_dir_cap, ui_config_file),
            (ui_state_dir_cap, ui_state_file),
            colors,
            toggle_tag,
          ))
        },
        |id, cap| Box::new(TermUi::new(id, cap, views, ui_state)),
      );

      let screen = stdout()
        .lock()
        .into_alternate_screen()
        .unwrap()
        .into_raw_mode()
        .unwrap();
      let renderer = TermUiRenderer::new(screen, colors).unwrap();

      let () = b.iter(|| {
        let () = ui.render(&renderer);
      });
    });
  }
}
