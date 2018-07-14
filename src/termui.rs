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

use std::any::Any;
use std::io::Result;

use gui::Cap;
use gui::Event;
use gui::Handleable;
use gui::Id;
use gui::Key;
use gui::MetaEvent;
use gui::UiEvent;

use controller::Controller;
use event::EventUpdated;
use in_out::InOut;
use in_out::InOutArea;
use task_list_box::TaskListBox;
use tasks::Id as TaskId;
use tasks::Task;
use tasks::Tasks;


/// An implementation of a terminal based view.
#[derive(Debug, GuiRootWidget)]
pub struct TermUi {
  id: Id,
  in_out: Id,
  children: Vec<Id>,
  controller: Controller,
}


impl TermUi {
  /// Create a new view associated with the given controller.
  pub fn new(mut id: Id, cap: &mut Cap, controller: Controller) -> Result<Self> {
    let in_out = cap.add_widget(&mut id, &mut |parent, id, _cap| {
      Box::new(InOutArea::new(parent, id))
    });
    let task_list = cap.add_widget(&mut id, &mut |parent, id, _cap| {
      Box::new(TaskListBox::new(parent, id, in_out, controller.tasks()))
    });
    cap.focus(&task_list);

    Ok(TermUi {
      id: id,
      in_out: in_out,
      children: Vec::new(),
      controller: controller,
    })
  }

  /// Save the current state.
  fn save(&mut self) -> MetaEvent {
    let in_out = match self.controller.save() {
      Ok(_) => InOut::Saved,
      Err(err) => InOut::Error(format!("{}", err)),
    };
    UiEvent::Custom(self.in_out, Box::new(in_out)).into()
  }

  fn handle_tasks_event(&mut self, data: &Box<Any>) -> Option<Option<MetaEvent>> {
    if let Some(_) = data.downcast_ref::<Tasks>() {
      let query = self.controller.tasks();
      let tasks = Tasks::from(query.collect::<Vec<Task>>());
      Some(Some(Event::Custom(Box::new(tasks)).into()))
    } else {
      None
    }
  }

  fn handle_task_remove_event(&mut self, data: &Box<Any>) -> Option<Option<MetaEvent>> {
    if let Some(task_id) = data.downcast_ref::<TaskId>() {
      self.controller.remove_task(*task_id);
      Some(None)
    } else {
      None
    }
  }

  fn handle_in_out_event(&mut self, data: &Box<Any>) -> Option<Option<MetaEvent>> {
    if let Some(in_out) = data.downcast_ref::<InOut>() {
      match in_out {
        InOut::Input(s) => self.controller.add_task(Task::new(s.clone())),
        _ => panic!("Unexpected input/output message: {:?}", in_out),
      };
      Some((None as Option<Event>).update())
    } else {
      None
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(&mut self, data: &Box<Any>) -> Option<Option<MetaEvent>> {
    None
      .or_else(|| self.handle_tasks_event(data))
      .or_else(|| self.handle_task_remove_event(data))
      .or_else(|| self.handle_in_out_event(data))
  }
}

impl Handleable for TermUi {
  /// Check for new input and react to it.
  fn handle(&mut self, event: Event, _cap: &mut Cap) -> Option<MetaEvent> {
    match event {
      Event::KeyDown(key) |
      Event::KeyUp(key) => {
        match key {
          Key::Char('q') => Some(UiEvent::Quit.into()),
          Key::Char('w') => Some(self.save()),
          _ => Some(event.into()),
        }
      },
      Event::Custom(data) => {
        match self.handle_custom_event(&data) {
          Some(x) => x,
          // If the event did not get handled we bubble it up.
          None => Some(Event::Custom(data).into()),
        }
      },
    }
  }
}


#[cfg(test)]
mod tests {
  use std::iter::FromIterator;

  use super::*;

  use gui::Ui;

  use tasks::Tasks;
  use tasks::tests::make_tasks;
  use tasks::tests::NamedTempFile;


  /// Test function for the TermUi.
  ///
  /// Instantiate the TermUi in a mock environment associating it with
  /// the given task list, supply the given input, and retrieve the
  /// resulting set of tasks.
  fn test(tasks: Tasks, events: Vec<UiEvent>) -> Tasks {
    let file = NamedTempFile::new();
    tasks.save(&*file).unwrap();
    let (mut ui, _) = Ui::new(&mut |id, cap| {
      let controller = Controller::new((*file).clone()).unwrap();
      Box::new(TermUi::new(id, cap, controller).unwrap())
    });

    for event in events {
      if let Some(UiEvent::Quit) = ui.handle(event) {
        break
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


  #[test]
  fn exit_on_quit() {
    let tasks = make_tasks(0);
    let events = vec![
      Event::KeyDown(Key::Char('q')).into(),
    ];

    assert_eq!(test(tasks, events), Tasks::from(make_tasks(0)))
  }

  #[test]
  fn remove_no_task() {
    let tasks = make_tasks(0);
    let events = vec![
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];

    assert_eq!(test(tasks, events), make_tasks(0))
  }

  #[test]
  fn remove_only_task() {
    let tasks = make_tasks(1);
    let events = vec![
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];

    assert_eq!(test(tasks, events), make_tasks(0))
  }

  #[test]
  fn remove_task_after_down_select() {
    let tasks = make_tasks(2);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];

    assert_eq!(test(tasks, events), make_tasks(1))
  }

  #[test]
  fn remove_task_after_up_select() {
    let tasks = make_tasks(3);
    let mut expected = make_tasks(3);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];

    let id = expected.iter().nth(1).unwrap().id;
    expected.remove(id);
    assert_eq!(test(tasks, events), expected)
  }

  #[test]
  fn remove_last_task() {
    let tasks = make_tasks(3);
    let mut expected = make_tasks(3);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let id = expected.iter().next().unwrap().id;
    expected.remove(id);
    let id = expected.iter().nth(1).unwrap().id;
    expected.remove(id);

    assert_eq!(test(tasks, events), expected)
  }

  #[test]
  fn add_task() {
    let tasks = make_tasks(0);
    let events = vec![
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('f')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Char('b')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('r')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];
    let expected = Tasks::from_iter(
      vec![
        Task::new("foobar".to_string())
      ].iter().cloned()
    );

    assert_eq!(test(tasks, events), expected)
  }

  #[test]
  fn add_and_remove_tasks() {
    let tasks = make_tasks(0);
    let events = vec![
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('f')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('b')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('r')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];
    let expected = Tasks::from_iter(Vec::new());

    assert_eq!(test(tasks, events), expected)
  }

  #[test]
  fn add_task_cancel() {
    let tasks = make_tasks(0);
    let events = vec![
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('f')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Char('b')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('z')).into(),
      Event::KeyDown(Key::Esc).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];
    let expected = make_tasks(0);

    assert_eq!(test(tasks, events), expected)
  }

  #[test]
  fn add_task_with_character_removal() {
    let tasks = Tasks::from(make_tasks(1));
    let events = vec![
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('f')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Backspace).into(),
      Event::KeyDown(Key::Backspace).into(),
      Event::KeyDown(Key::Backspace).into(),
      Event::KeyDown(Key::Char('b')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('z')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];
    let mut expected = make_tasks(1);
    expected.add(Task::new("baz".to_string()));

    assert_eq!(test(tasks, events), expected)
  }
}
