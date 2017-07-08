// termui.rs

// *************************************************************************
// * Copyright (C) 2017 Daniel Mueller (deso@posteo.net)                   *
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

use std::cmp::max;
use std::cmp::min;
use std::io::BufWriter;
use std::io::Read;
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
use termion::event::Event;
use termion::event::Key;
use termion::input::Events;
use termion::input::TermRead;
use termion::terminal_size;

use controller::Controller;
use view::Quit;
use view::Result;
use view::View;


const MAIN_MARGIN_X: u16 = 3;
const MAIN_MARGIN_Y: u16 = 2;
const TASK_SPACE: u16 = 2;

// TODO: Make the colors run time configurable at some point.
// Color 0.
const UNSELECTED_TASK_FG: &Rgb = &Rgb(0x00, 0x00, 0x00);
/// The terminal default background.
const UNSELECTED_TASK_BG: &Reset = &Reset;
/// Color 15.
const SELECTED_TASK_FG: &Rgb = &Rgb(0xff, 0xff, 0xff);
/// Color 240.
const SELECTED_TASK_BG: &Rgb = &Rgb(0x58, 0x58, 0x58);


/// An implementation of a terminal based view.
pub struct TermUi<'ctrl, R, W>
where
  R: Read,
  W: Write,
{
  writer: BufWriter<W>,
  events: Events<R>,
  controller: &'ctrl mut Controller,
  offset: isize,
  selection: isize,
}


impl<'ctrl, R, W> TermUi<'ctrl, R, W>
where
  R: Read,
  W: Write,
{
  /// Create a new view associated with the given controller.
  pub fn new(reader: R, writer: W, controller: &'ctrl mut Controller) -> Result<Self> {
    let events = reader.events();
    // Compared to termbox termion suffers from flickering when clearing
    // the entire screen as it lacks any double buffering capabilities
    // and uses an escape sequence for the clearing. One proposed
    // solution is to use a io::BufWriter. Tests have shown that it does
    // not change much but conceptually it makes sense to use it
    // nevertheless -- so this is what we do. For a broader discussion
    // of this issue see https://github.com/ticki/termion/issues/105.
    let mut writer = BufWriter::new(writer);

    write!(writer, "{}{}", Hide, All)?;

    Ok(TermUi {
      writer: writer,
      events: events,
      controller: controller,
      offset: 0,
      selection: 0,
    })
  }

  /// Change the currently selected task.
  fn select(&mut self, change: isize) {
    self.selection += change;
  }

  /// Retrieve the number of tasks that fit on the screen.
  fn displayable_tasks(&self) -> Result<isize> {
    let (_, h) = terminal_size()?;
    Ok(((h - MAIN_MARGIN_Y) / TASK_SPACE) as isize)
  }

  /// Sanitize selection and offset about what is being displayed.
  ///
  /// This functionality is to be called from inside the "update" path
  /// where we visualize the current data.
  fn sanitize_view<T>(&self, iter: T) -> Result<(isize, isize, isize)>
  where
    T: Iterator,
  {
    let task_limit = self.displayable_tasks()?;
    let task_count = iter.count() as isize;

    let mut select = self.selection;
    let mut offset = self.offset;

    select = max(0, min(task_count - 1, select));
    offset = min(select, max(offset, select - (task_limit - 1)));

    Ok((offset, select, task_limit))
  }

  /// Update, i.e., redraw, all the tasks.
  fn update_tasks(&mut self) -> Result<()> {
    let x = MAIN_MARGIN_X;
    let mut y = MAIN_MARGIN_Y;

    let iter = self.controller.tasks();
    let (offset, selection, limit) = self.sanitize_view(iter.clone())?;

    for (i, task) in iter.enumerate().skip(offset as usize) {
      if i as isize >= offset + limit {
        break
      }

      let (fg, bg) = if i as isize == selection {
        (SELECTED_TASK_FG as &Color, SELECTED_TASK_BG as &Color)
      } else {
        (UNSELECTED_TASK_FG as &Color, UNSELECTED_TASK_BG as &Color)
      };

      write!(
        self.writer,
        "{}{}{}{}",
        Goto(x + 1, y + 1),
        Fg(fg),
        Bg(bg),
        task.summary
      )?;
      y += TASK_SPACE;
    }

    self.offset = offset;
    self.selection = selection;

    Ok(())
  }
}


impl<'ctrl, R, W> View for TermUi<'ctrl, R, W>
where
  R: Read,
  W: Write,
{
  /// Check for new input and react to it.
  fn poll(&mut self) -> Result<Quit> {
    if let Some(event) = self.events.next() {
      let needs_update = match event? {
        Event::Key(key) => {
          match key {
            Key::Char('j') => {
              self.select(1);
              true
            },
            Key::Char('k') => {
              self.select(-1);
              true
            },
            Key::Char('q') => return Ok(Quit::Yes),
            _ => false,
          }
        },
        _ => false,
      };

      if needs_update {
        self.update()?
      }
    }
    Ok(Quit::No)
  }

  /// Update the view by redrawing the user interface.
  fn update(&mut self) -> Result<()> {
    write!(
      self.writer,
      "{}{}{}",
      Fg(Reset),
      Bg(Reset),
      All
    )?;
    self.update_tasks()?;
    self.writer.flush()?;
    Ok(())
  }
}


impl<'ctrl, R, W> Drop for TermUi<'ctrl, R, W>
where
  R: Read,
  W: Write,
{
  fn drop(&mut self) {
    let _ = write!(self.writer, "{}", Show);
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use tasks::TaskIter;
  use tasks::Tasks;
  use tasks::tests::make_tasks;
  use view::Quit;


  #[derive(Debug)]
  struct MockController {
    tasks: Tasks,
  }

  impl MockController {
    fn new(tasks: Tasks) -> Self {
      Self {
        tasks: tasks,
      }
    }
  }

  impl Into<Tasks> for MockController {
    fn into(self) -> Tasks {
      self.tasks
    }
  }

  impl Controller for MockController {
    fn tasks(&self) -> TaskIter {
      self.tasks.iter()
    }
  }


  // Test function for the TermUi.
  //
  // Instantiate the TermUi in a mock environment associating it with
  // the given task list, supply the given input, and retrieve the
  // resulting set of tasks.
  fn test<R>(tasks: Tasks, input: R) -> Tasks
  where
    R: Read,
  {
    let mut controller = MockController::new(tasks);

    {
      let writer = vec![];
      let mut ui = TermUi::new(input, writer, &mut controller).unwrap();

      loop {
        if let Quit::Yes = ui.poll().unwrap() {
          break
        }
      }
    }

    controller.into()
  }


  #[test]
  fn exit_on_quit() {
    let tasks = make_tasks(0);
    let input = String::from("q");

    assert_eq!(test(tasks, input.as_bytes()), make_tasks(0))
  }
}
