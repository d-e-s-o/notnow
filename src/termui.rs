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
use tab_bar::TabBar;
use task_list_box::TaskListBox;
use tasks::Id as TaskId;
use tasks::Task;


/// An enumeration comprising all custom events we support.
#[derive(Debug)]
pub enum TermUiEvent {
  /// Add a task with the given summary.
  AddTask(String),
  /// Remove the task with the given ID.
  RemoveTask(TaskId),
  /// Update the given task.
  UpdateTask(Task),
  /// Set the state of the input/output area.
  SetInOut(InOut),
  /// A indication that some component changed and that we should
  /// re-render everything.
  Updated,
  /// Retrieve the current set of tasks.
  #[cfg(test)]
  GetTasks,
}

impl TermUiEvent {
  /// Check whether the event is the `Updated` variant.
  #[cfg(test)]
  pub fn is_updated(&self) -> bool {
    if let TermUiEvent::Updated = self {
      true
    } else {
      false
    }
  }
}


/// An implementation of a terminal based view.
#[derive(Debug, GuiWidget)]
pub struct TermUi {
  id: Id,
  in_out: Id,
  controller: Controller,
}


impl TermUi {
  /// Create a new view associated with the given controller.
  pub fn new(id: Id, cap: &mut Cap, controller: Controller) -> Result<Self> {
    let in_out = cap.add_widget(id, &mut |id, _cap| {
      Box::new(InOutArea::new(id))
    });
    let tab_bar = cap.add_widget(id, &mut |id, _cap| {
      Box::new(TabBar::new(id))
    });
    // Careful with the ordering of children here. The TaskListBox
    // rendering process sets the cursor to the currently selected task.
    // That will only work if this widget is the one being rendered
    // last.
    let task_list = cap.add_widget(tab_bar, &mut |id, _cap| {
      Box::new(TaskListBox::new(id, in_out, controller.tasks()))
    });
    cap.focus(task_list);

    Ok(TermUi {
      id: id,
      in_out: in_out,
      controller: controller,
    })
  }

  /// Save the current state.
  fn save(&mut self) -> MetaEvent {
    let in_out = match self.controller.save() {
      Ok(_) => InOut::Saved,
      Err(err) => InOut::Error(format!("{}", err)),
    };
    let event = TermUiEvent::SetInOut(in_out);
    UiEvent::Custom(self.in_out, Box::new(event)).into()
  }

  /// Handle a custom event.
  fn handle_custom_event(&mut self, event: Box<TermUiEvent>) -> Option<MetaEvent> {
    match *event {
      TermUiEvent::AddTask(s) => {
        self.controller.add_task(s);
        (None as Option<Event>).update()
      },
      TermUiEvent::RemoveTask(id) => {
        self.controller.remove_task(id);
        (None as Option<Event>).update()
      },
      TermUiEvent::UpdateTask(task) => {
        self.controller.update_task(task);
        (None as Option<Event>).update()
      },
      #[cfg(test)]
      TermUiEvent::GetTasks => {
        let tasks = self.controller.tasks().collect::<Vec<Task>>();
        Some(Event::Custom(Box::new(tasks)).into())
      },
      _ => Some(Event::Custom(event).into()),
    }
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
        match data.downcast::<TermUiEvent>() {
          Ok(e) => self.handle_custom_event(e),
          Err(e) => panic!("Received unexpected custom event: {:?}", e),
        }
      },
    }
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use gui::Ui;

  use tasks::Tasks;
  use tasks::tests::make_tasks_vec;
  use tasks::tests::NamedTempFile;
  use tasks::tests::TaskVec;


  /// Test function for the TermUi.
  ///
  /// Instantiate the TermUi in a mock environment associating it with
  /// the given task list, supply the given input, and retrieve the
  /// resulting set of tasks.
  fn test(task_vec: TaskVec, events: Vec<UiEvent>) -> TaskVec {
    let mut tasks = Some(Tasks::from(task_vec));
    let file = NamedTempFile::new();
    let (mut ui, _) = Ui::new(&mut |id, cap| {
      let tasks = tasks.take().unwrap();
      let controller = Controller::with_tasks_and_path(tasks, file.path());
      Box::new(TermUi::new(id, cap, controller).unwrap())
    });

    for event in events {
      if let Some(UiEvent::Quit) = ui.handle(event) {
        break
      }
    }

    let event = Event::Custom(Box::new(TermUiEvent::GetTasks));
    let response = ui.handle(event).unwrap();
    match response {
      UiEvent::Event(event) => {
        match event {
          Event::Custom(x) => TaskVec(*x.downcast::<Vec<Task>>().unwrap()),
          _ => panic!("Unexpected event: {:?}", event),
        }
      },
      _ => panic!("Unexpected event: {:?}", response),
    }
  }


  #[test]
  fn exit_on_quit() {
    let tasks = make_tasks_vec(0);
    let events = vec![
      Event::KeyDown(Key::Char('q')).into(),
    ];

    assert_eq!(test(tasks, events), make_tasks_vec(0))
  }

  #[test]
  fn remove_no_task() {
    let tasks = make_tasks_vec(0);
    let events = vec![
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];

    assert_eq!(test(tasks, events), make_tasks_vec(0))
  }

  #[test]
  fn remove_only_task() {
    let tasks = make_tasks_vec(1);
    let events = vec![
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];

    assert_eq!(test(tasks, events), make_tasks_vec(0))
  }

  #[test]
  fn remove_task_after_down_select() {
    let tasks = make_tasks_vec(2);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];

    assert_eq!(test(tasks, events), make_tasks_vec(1))
  }

  #[test]
  fn remove_task_after_up_select() {
    let tasks = make_tasks_vec(3);
    let mut expected = make_tasks_vec(3);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];

    expected.remove(1);
    assert_eq!(test(tasks, events), expected)
  }

  #[test]
  fn remove_last_task() {
    let tasks = make_tasks_vec(3);
    let mut expected = make_tasks_vec(3);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('d')).into(),
    ];

    expected.remove(0);
    expected.remove(1);

    assert_eq!(test(tasks, events), expected)
  }

  #[test]
  fn add_task() {
    let tasks = make_tasks_vec(0);
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
    let expected = TaskVec(vec![
      Task::new("foobar")
    ]);

    assert_eq!(test(tasks, events), expected)
  }

  #[test]
  fn add_and_remove_tasks() {
    let tasks = make_tasks_vec(0);
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
    let expected = TaskVec(Vec::new());

    assert_eq!(test(tasks, events), expected)
  }

  #[test]
  fn add_task_cancel() {
    let tasks = make_tasks_vec(0);
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
    let expected = make_tasks_vec(0);

    assert_eq!(test(tasks, events), expected)
  }

  #[test]
  fn add_task_with_character_removal() {
    let tasks = make_tasks_vec(1);
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
    let mut expected = make_tasks_vec(1);
    expected.push(Task::new("baz"));

    assert_eq!(test(tasks, events), expected)
  }

  #[test]
  fn complete_task() {
    let tasks = make_tasks_vec(3);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char(' ')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char(' ')).into(),
      Event::KeyDown(Key::Char(' ')).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];

    let tasks = test(tasks, events);
    assert!(!tasks[0].is_complete());
    assert!(tasks[1].is_complete());
    assert!(!tasks[2].is_complete());
  }
}
