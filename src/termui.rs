// termui.rs

// *************************************************************************
// * Copyright (C) 2017-2018 Daniel Mueller (deso@posteo.net)              *
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
use termion::terminal_size;

use controller::Controller;
use tasks::Task;
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

const SAVED_TEXT: &str = " Saved ";
const ERROR_TEXT: &str = " Error ";
const INPUT_TEXT: &str = " > ";


/// An object representing the in/out area within the TermUi.
enum InOutArea {
  Saved,
  Error(String),
  Input(String),
  Clear,
}


type Handler<'ctrl, W> = fn(obj: &mut TermUi<'ctrl, W>, event: &Event) -> Result<Quit>;


/// An implementation of a terminal based view.
pub struct TermUi<'ctrl, W>
where
  W: Write,
{
  writer: BufWriter<W>,
  controller: &'ctrl mut Controller,
  handler: Handler<'ctrl, W>,
  in_out: InOutArea,
  offset: isize,
  selection: isize,
}


impl<'ctrl, W> TermUi<'ctrl, W>
where
  W: Write,
{
  /// Create a new view associated with the given controller.
  pub fn new(writer: W, controller: &'ctrl mut Controller) -> Result<Self> {
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
      controller: controller,
      handler: TermUi::<'ctrl, W>::handle_event,
      in_out: InOutArea::Clear,
      offset: 0,
      selection: 0,
    })
  }

  /// Save the current state.
  fn save(&mut self) {
    match self.controller.save() {
      Ok(_) => self.in_out = InOutArea::Saved,
      Err(err) => self.in_out = InOutArea::Error(format!("{}", err)),
    }
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

  fn update_input_output(&mut self) -> Result<()> {
    let x = 0;
    let (_, y) = terminal_size()?;

    let (prefix, fg, bg, string) = match self.in_out {
      InOutArea::Saved => (Some(SAVED_TEXT), IN_OUT_SUCCESS_FG, IN_OUT_SUCCESS_BG, None),
      InOutArea::Error(ref e) => (Some(ERROR_TEXT), IN_OUT_ERROR_FG, IN_OUT_ERROR_BG, Some(e)),
      InOutArea::Input(ref s) => (Some(INPUT_TEXT), IN_OUT_SUCCESS_FG, IN_OUT_SUCCESS_BG, Some(s)),
      InOutArea::Clear => return Ok(()),
    };

    write!(self.writer, "{}", Goto(x + 1, y + 1))?;

    if let Some(prefix) = prefix {
      write!(self.writer, "{}{}{}", Fg(*fg), Bg(*bg), prefix)?;
    }
    if let Some(string) = string {
      write!(
        self.writer,
        "{}{} {}",
        Fg(*IN_OUT_STRING_FG),
        Bg(*IN_OUT_STRING_BG),
        string
      )?;
    }
    Ok(())
  }

  fn handle_event(&mut self, event: &Event) -> Result<Quit> {
    let needs_update = match *event {
      Event::Key(key) => {
        let update = if let InOutArea::Clear = self.in_out {
          false
        } else {
          // We clear the input/output area after any key event.
          self.in_out = InOutArea::Clear;
          true
        };

        match key {
          Key::Char('a') => {
            self.in_out = InOutArea::Input("".to_string());
            self.handler = Self::handle_input;
            true
          },
          Key::Char('d') => {
            let count = self.controller.tasks().count();
            if count > 0 {
              self.controller.remove_task(self.selection as usize);
              self.select(0);
              // We have removed a task. Always indicate that an update
              // is necessary here.
              true
            } else {
              false
            }
          },
          Key::Char('j') => {
            self.select(1);
            true
          },
          Key::Char('k') => {
            self.select(-1);
            true
          },
          Key::Char('q') => return Ok(Quit::Yes),
          Key::Char('w') => {
            self.save();
            true
          },
          _ => update,
        }
      },
      _ => false,
    };

    if needs_update {
      self.update()?
    }
    Ok(Quit::No)
  }

  fn handle_input(&mut self, event: &Event) -> Result<Quit> {
    let needs_update = match *event {
      Event::Key(key) => {
        match key {
          Key::Char('\n') => {
            if let InOutArea::Input(ref s) = self.in_out {
              self.controller.add_task(Task {
                summary: s.clone(),
              });
            } else {
              panic!("In/out area not used for input.");
            }
            self.in_out = InOutArea::Clear;
            self.handler = Self::handle_event;
            true
          },
          Key::Char(c) => {
            self.in_out = InOutArea::Input(if let InOutArea::Input(ref mut s) = self.in_out {
              s.push(c);
              s.clone()
            } else {
              panic!("In/out area not used for input.");
            });
            true
          },
          Key::Esc => {
            self.in_out = InOutArea::Clear;
            self.handler = Self::handle_event;
            true
          },
          _ => false,
        }
      },
      _ => false,
    };

    if needs_update {
      self.update()?
    }
    Ok(Quit::No)
  }
}


impl<'ctrl, W> View for TermUi<'ctrl, W>
where
  W: Write,
{
  /// Check for new input and react to it.
  fn handle(&mut self, event: &Event) -> Result<Quit> {
    (self.handler)(self, event)
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
    self.update_input_output()?;
    self.writer.flush()?;
    Ok(())
  }
}


impl<'ctrl, W> Drop for TermUi<'ctrl, W>
where
  W: Write,
{
  fn drop(&mut self) {
    let _ = write!(self.writer, "{}", Show);
  }
}


#[cfg(test)]
mod tests {
  use std::io;
  use std::iter::FromIterator;

  use termion::input::TermRead;

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
    fn save(&self) -> io::Result<()> {
      // No-op.
      Ok(())
    }

    fn tasks(&self) -> TaskIter {
      self.tasks.iter()
    }

    fn add_task(&mut self, task: Task) {
      self.tasks.add(task)
    }

    fn remove_task(&mut self, index: usize) {
      self.tasks.remove(index)
    }
  }


  /// Test function for the TermUi that supports inputs of the escape key.
  ///
  /// This function performs the same basic task as `test` but it
  /// supports multiple input streams. By doing so we implicitly support
  /// passing in input that contains an escape key sequence. Handling of
  /// the escape key in termion is tricky: An escape in the traditional
  /// sense is just a byte with value 0x1b. Termion treats such a byte as
  /// Key::Esc only if it is not followed by any additional input. If
  /// additional bytes follow, 0x1b will just act as the introduction for
  /// an escape sequence.
  fn test_for_esc(tasks: Tasks, input: Vec<&[u8]>) -> Tasks {
    let mut controller = MockController::new(tasks);

    {
      let writer = vec![];
      let mut ui = TermUi::new(writer, &mut controller).unwrap();

      for data in input {
        let mut events = data.events();
        for event in events {
          if let Quit::Yes = ui.handle(&event.unwrap()).unwrap() {
            break
          }
        }
      }
    }

    controller.into()
  }

  /// Test function for the TermUi.
  ///
  /// Instantiate the TermUi in a mock environment associating it with
  /// the given task list, supply the given input, and retrieve the
  /// resulting set of tasks.
  fn test(tasks: Tasks, input: &[u8]) -> Tasks {
    test_for_esc(tasks, vec![input])
  }


  #[test]
  fn exit_on_quit() {
    let tasks = make_tasks(0);
    let input = String::from("q");

    assert_eq!(test(tasks, input.as_bytes()), make_tasks(0))
  }

  #[test]
  fn remove_no_task() {
    let tasks = make_tasks(0);
    let input = String::from("dq");

    assert_eq!(test(tasks, input.as_bytes()), make_tasks(0))
  }

  #[test]
  fn remove_only_task() {
    let tasks = make_tasks(1);
    let input = String::from("dq");

    assert_eq!(test(tasks, input.as_bytes()), make_tasks(0))
  }

  #[test]
  fn remove_task_after_down_select() {
    let tasks = make_tasks(2);
    let input = String::from("jjjjjdq");

    assert_eq!(test(tasks, input.as_bytes()), make_tasks(1))
  }

  #[test]
  fn remove_task_after_up_select() {
    let tasks = make_tasks(3);
    let mut expected = make_tasks(3);
    let input = String::from("jjkdq");

    expected.remove(1);
    assert_eq!(test(tasks, input.as_bytes()), expected)
  }

  #[test]
  fn add_task() {
    let tasks = make_tasks(0);
    let input = String::from("afoobar\nq");
    let expected = Tasks::from_iter(
      vec![
        Task {
          summary: "foobar".to_string(),
        },
      ].iter().cloned()
    );

    assert_eq!(test(tasks, input.as_bytes()), expected)
  }

  #[test]
  fn add_and_remove_tasks() {
    let tasks = make_tasks(0);
    let input = String::from("afoo\nabar\nddq");
    let expected = Tasks::from_iter(Vec::new());

    assert_eq!(test(tasks, input.as_bytes()), expected)
  }

  #[test]
  fn add_task_cancel() {
    let tasks = make_tasks(0);
    let input1 = String::from("afoobaz\x1b");
    let input2 = String::from("q");
    let input = vec![input1.as_bytes(), input2.as_bytes()];
    let expected = make_tasks(0);

    assert_eq!(test_for_esc(tasks, input), expected)
  }
}
