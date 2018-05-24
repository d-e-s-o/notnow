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
use std::fmt::Debug;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::io::Result;
use std::iter::FromIterator;
use std::ops::Deref;

use gui::Cap;
use gui::ChildIter;
use gui::Event;
use gui::Handleable;
use gui::Id;
use gui::Key;
use gui::MetaEvent;
use gui::Object;
use gui::Renderable;
use gui::Renderer;
use gui::Ui;
use gui::UiEvent;
use gui::Widget;
use gui::WidgetRef;

use controller::Controller;
use tasks::Task;
use tasks::Tasks;


/// Sanitize a selection index.
fn sanitize_selection(selection: isize, count: usize) -> usize {
  max(0, min(count as isize - 1, selection)) as usize
}


/// An object representing the in/out area within the TermUi.
#[derive(Debug)]
pub enum InOutArea {
  Saved,
  Error(String),
  Input(String),
  Clear,
}


type HandlerFn = fn(obj: &mut TermUi, event: Event) -> Option<MetaEvent>;

struct Handler(HandlerFn);

impl Debug for Handler {
  fn fmt(&self, f: &mut Formatter) -> FmtResult {
    write!(f, "notnow::termui::Handler")
  }
}

impl Deref for Handler {
  type Target = HandlerFn;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}


/// An implementation of a terminal based view.
#[derive(Debug)]
pub struct TermUi {
  id: Id,
  children: Vec<Id>,
  controller: Controller,
  handler: Handler,
  in_out: InOutArea,
  offset: Cell<usize>,
  selection: usize,
  update: bool,
}


impl TermUi {
  /// Create a new view associated with the given controller.
  pub fn new(id: Id, controller: Controller) -> Result<Self> {
    Ok(TermUi {
      id: id,
      children: Vec::new(),
      controller: controller,
      handler: Handler(TermUi::handle_event),
      in_out: InOutArea::Clear,
      offset: Cell::new(0),
      selection: 0,
      update: true,
    })
  }

  /// Retrieve the current selection index.
  ///
  /// The selection index indicates the currently selected task.
  pub fn selection(&self) -> usize {
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
    let old_selection = self.selection;
    let new_selection = self.selection as isize + change;
    self.selection = sanitize_selection(new_selection, count);

    self.selection != old_selection
  }

  fn handle_event(&mut self, event: Event) -> Option<MetaEvent> {
    let (result, update) = match event {
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
            self.handler = Handler(Self::handle_input);
            (None, true)
          },
          Key::Char('d') => {
            let count = self.controller.tasks().count();
            if count > 0 {
              self.controller.remove_task(self.selection);
              self.select(0);
              // We have removed a task. Always indicate that an update
              // is necessary here.
              (None, true)
            } else {
              (None, update)
            }
          },
          Key::Char('j') => (None, self.select(1)),
          Key::Char('k') => (None, self.select(-1)),
          Key::Char('q') => return Some(UiEvent::Quit.into()),
          Key::Char('w') => {
            self.save();
            (None, true)
          },
          _ => (Some(UiEvent::Event(event).into()), update),
        }
      },
      Event::Custom(data) => {
        match data.downcast::<Tasks>() {
          Ok(_) => {
            let tasks = Tasks::from_iter(self.controller.tasks().cloned());
            (Some(Event::Custom(Box::new(tasks)).into()), false)
          },
          Err(e) => (Some(Event::Custom(e).into()), false),
        }
      },
    };

    self.update = update;
    result
  }

  fn handle_input(&mut self, event: Event) -> Option<MetaEvent> {
    let (result, update) = match event {
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
            self.handler = Handler(Self::handle_event);
            (None, true)
          },
          Key::Char(c) => {
            self.in_out = InOutArea::Input(if let InOutArea::Input(ref mut s) = self.in_out {
              s.push(c);
              s.clone()
            } else {
              panic!("In/out area not used for input.");
            });
            (None, true)
          },
          Key::Esc => {
            self.in_out = InOutArea::Clear;
            self.handler = Handler(Self::handle_event);
            (None, true)
          },
          _ => (None, false),
        }
      },
      Event::Custom(x) => (Some(Event::Custom(x).into()), false),
    };

    self.update = update;
    result
  }
}

impl Renderable for TermUi {
  /// Render the user interface.
  fn render(&self, renderer: &Renderer) {
    // TODO: The conditional redraw logic we have here does not work
    //       as-is for a widget. The reason is that the `Ui` invokes the
    //       renderer's `pre_render` method without knowledge of whether
    //       any of the components changed. As this method always clears
    //       the screen we also always need to redraw. This will need to
    //       be fixed in the future.
    if true || self.update {
      renderer.render(self);
      renderer.render(&self.in_out);
    }
  }
}

impl Object for TermUi {
  fn id(&self) -> Id {
    self.id
  }
  fn parent_id(&self) -> Option<Id> {
    None
  }
  fn add_child(&mut self, widget: &WidgetRef) {
    self.children.push(widget.as_id())
  }
  fn iter(&self) -> ChildIter {
    ChildIter::with_iter(self.children.iter())
  }
}

impl WidgetRef for TermUi {
  fn as_widget<'s, 'ui: 's>(&'s self, _ui: &'ui Ui) -> &Widget {
    self
  }
  fn as_mut_widget<'s, 'ui: 's>(&'s mut self, _ui: &'ui mut Ui) -> &mut Widget {
    self
  }
  fn as_id(&self) -> Id {
    self.id()
  }
}

impl Widget for TermUi {}

impl Handleable for TermUi {
  /// Check for new input and react to it.
  fn handle(&mut self, event: Event, _cap: &mut Cap) -> Option<MetaEvent> {
    (self.handler)(self, event)
  }
}


#[cfg(test)]
mod tests {
  use std::iter::FromIterator;
  use std::str::Chars;

  use super::*;

  use gui::Ui;

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
    let (mut ui, _) = Ui::new(&mut |id, _cap| {
      let controller = Controller::new((*file).clone()).unwrap();
      Box::new(TermUi::new(id, controller).unwrap())
    });

    for data in input {
      for byte in data {
        let event = Event::KeyDown(Key::Char(byte));
        if let Some(UiEvent::Quit) = ui.handle(event) {
          break
        }
      }
    }

    let tasks = Tasks::from_iter(Vec::new().iter().cloned());
    let event = Event::Custom(Box::new(tasks));
    let response = ui.handle(event).unwrap();
    match response {
      UiEvent::Event(event) => {
        match event {
          Event::Custom(x) => *x.downcast::<Tasks>().unwrap(),
          _ => panic!("Unexpected event: {:?}", event),
        }
      },
      _ => panic!("Unexpected event: {:?}", response),
    }
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
