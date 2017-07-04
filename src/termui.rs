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

use std::io::BufWriter;
use std::io::Read;
use std::io::Write;

use termion::clear::All;
use termion::cursor::Hide;
use termion::cursor::Show;
use termion::event::Event;
use termion::event::Key;
use termion::input::Events;
use termion::input::TermRead;

use controller::Controller;
use view::Quit;
use view::Result;
use view::View;


/// An implementation of a terminal based view.
pub struct TermUi<'ctrl, R, W>
where
  R: Read,
  W: Write,
{
  writer: BufWriter<W>,
  events: Events<R>,
  #[allow(dead_code)]
  controller: &'ctrl mut Controller,
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
    })
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
      match event? {
        Event::Key(key) => {
          match key {
            Key::Char('q') => return Ok(Quit::Yes),
            _ => (),
          }
        },
        _ => (),
      }
    }
    Ok(Quit::No)
  }

  /// Update the view by redrawing the user interface.
  fn update(&mut self) -> Result<()> {
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
