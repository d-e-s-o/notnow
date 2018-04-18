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

use std::cell::Cell;
use std::cmp::max;
use std::cmp::min;
use std::io::Result;

use gui::Event;
use gui::Key;
use gui::Renderer;
use gui::UiEvent;

use controller::Controller;
use tasks::Task;


/// Sanitize a selection index.
pub fn sanitize_selection(selection: isize, count: usize) -> usize {
  max(0, min(count as isize - 1, selection)) as usize
}

/// Sanitize an offset.
pub fn sanitize_offset(offset: usize, selection: usize, limit: usize) -> usize {
  if selection <= offset {
    selection
  } else if selection > offset + (limit - 1) {
    selection - (limit - 1)
  } else {
    offset
  }
}


/// An object representing the in/out area within the TermUi.
pub enum InOutArea {
  Saved,
  Error(String),
  Input(String),
  Clear,
}


type Handler = fn(obj: &mut TermUi, event: &Event) -> Option<UiEvent>;


/// An implementation of a terminal based view.
pub struct TermUi {
  controller: Controller,
  handler: Handler,
  in_out: InOutArea,
  offset: Cell<usize>,
  selection: isize,
  update: bool,
}


impl TermUi {
  /// Create a new view associated with the given controller.
  pub fn new(controller: Controller) -> Result<Self> {
    Ok(TermUi {
      controller: controller,
      handler: TermUi::handle_event,
      in_out: InOutArea::Clear,
      offset: Cell::new(0),
      selection: 0,
      update: true,
    })
  }

  /// Retrieve the current selection index.
  ///
  /// The selection index indicates the currently selected task. Note
  /// that for various reasons such as resizing events the returned
  /// index should be sanitized via `sanitize_selection` before usage.
  pub fn selection(&self) -> isize {
    self.selection
  }

  /// Retrieve the current view offset.
  ///
  /// The offset indicates the task at which to start displaying. Not
  /// that for various reasons such as resizing events the returned
  /// index should be sanitized via `sanitize_offset` before usage.
  pub fn offset(&self) -> usize {
    self.offset.get()
  }

  /// Adjust the view offset to use.
  pub fn reoffset(&self, offset: usize) {
    self.offset.set(offset)
  }

  /// Retrieve the UI's controller.
  pub fn controller(&self) -> &Controller {
    &self.controller
  }

  /// Save the current state.
  fn save(&mut self) {
    match self.controller.save() {
      Ok(_) => self.in_out = InOutArea::Saved,
      Err(err) => self.in_out = InOutArea::Error(format!("{}", err)),
    }
  }

  /// Change the currently selected task.
  fn select(&mut self, change: isize) -> bool {
    let count = self.controller.tasks().count();
    let old_selection = sanitize_selection(self.selection, count) as isize;
    self.selection = sanitize_selection(self.selection + change, count) as isize;

    self.selection != old_selection
  }

  /// Render the user interface.
  pub fn render<R>(&mut self, renderer: &R)
  where
    R: Renderer,
  {
    if self.update {
      renderer.pre_render();
      renderer.render(self);
      renderer.render(&self.in_out);
      renderer.post_render();
    }
  }

  /// Check for new input and react to it.
  pub fn handle(&mut self, event: &Event) -> Option<UiEvent> {
    (self.handler)(self, event)
  }

  fn handle_event(&mut self, event: &Event) -> Option<UiEvent> {
    self.update = match *event {
      Event::KeyDown(key) |
      Event::KeyUp(key) => {
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
              let selection = sanitize_selection(self.selection, count);
              self.controller.remove_task(selection as usize);
              self.select(0);
              // We have removed a task. Always indicate that an update
              // is necessary here.
              true
            } else {
              false
            }
          },
          Key::Char('j') => self.select(1),
          Key::Char('k') => self.select(-1),
          Key::Char('q') => return Some(UiEvent::Quit),
          Key::Char('w') => {
            self.save();
            true
          },
          _ => update,
        }
      },
      _ => false,
    };
    None
  }

  fn handle_input(&mut self, event: &Event) -> Option<UiEvent> {
    self.update = match *event {
      Event::KeyDown(key) |
      Event::KeyUp(key) => {
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
    None
  }
}


#[cfg(test)]
mod tests {
  use std::iter::FromIterator;
  use std::str::Chars;

  use super::*;

  use tasks::Tasks;
  use tasks::tests::make_tasks;
  use tasks::tests::NamedTempFile;


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
  fn test_for_esc(tasks: Tasks, input: Vec<Chars>) -> Tasks {
    let file = NamedTempFile::new();
    tasks.save(&*file).unwrap();
    let controller = Controller::new((*file).clone()).unwrap();

    let mut ui = TermUi::new(controller).unwrap();

    for data in input {
      for byte in data {
        let event = Event::KeyDown(Key::Char(byte));
        if let Some(UiEvent::Quit) = ui.handle(&event) {
          break
        }
      }
    }

    Tasks::from_iter(ui.controller().tasks().cloned())
  }

  /// Test function for the TermUi.
  ///
  /// Instantiate the TermUi in a mock environment associating it with
  /// the given task list, supply the given input, and retrieve the
  /// resulting set of tasks.
  fn test(tasks: Tasks, input: Chars) -> Tasks {
    test_for_esc(tasks, vec![input])
  }


  #[test]
  fn exit_on_quit() {
    let tasks = make_tasks(0);
    let input = String::from("q");

    assert_eq!(test(tasks, input.chars()), make_tasks(0))
  }

  #[test]
  fn remove_no_task() {
    let tasks = make_tasks(0);
    let input = String::from("dq");

    assert_eq!(test(tasks, input.chars()), make_tasks(0))
  }

  #[test]
  fn remove_only_task() {
    let tasks = make_tasks(1);
    let input = String::from("dq");

    assert_eq!(test(tasks, input.chars()), make_tasks(0))
  }

  #[test]
  fn remove_task_after_down_select() {
    let tasks = make_tasks(2);
    let input = String::from("jjjjjdq");

    assert_eq!(test(tasks, input.chars()), make_tasks(1))
  }

  #[test]
  fn remove_task_after_up_select() {
    let tasks = make_tasks(3);
    let mut expected = make_tasks(3);
    let input = String::from("jjkdq");

    expected.remove(1);
    assert_eq!(test(tasks, input.chars()), expected)
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

    assert_eq!(test(tasks, input.chars()), expected)
  }

  #[test]
  fn add_and_remove_tasks() {
    let tasks = make_tasks(0);
    let input = String::from("afoo\nabar\nddq");
    let expected = Tasks::from_iter(Vec::new());

    assert_eq!(test(tasks, input.chars()), expected)
  }

  #[test]
  fn add_task_cancel() {
    let tasks = make_tasks(0);
    let input1 = String::from("afoobaz\x1b");
    let input2 = String::from("q");
    let input = vec![input1.chars(), input2.chars()];
    let expected = make_tasks(0);

    assert_eq!(test_for_esc(tasks, input), expected)
  }
}
