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

use event::EventUpdated;
use in_out::InOut;
use in_out::InOutArea;
use state::State;
use tab_bar::TabBar;
use tasks::Id as TaskId;
use tasks::Task;


/// An enumeration comprising all custom events we support.
#[derive(Debug)]
pub enum TermUiEvent {
  /// Add a task with the given summary.
  AddTask(String),
  /// The response to the `AddTask` event.
  AddTaskResp(TaskId),
  /// Remove the task with the given ID.
  RemoveTask(TaskId),
  /// Update the given task.
  UpdateTask(Task),
  /// Set the state of the input/output area.
  SetInOut(InOut),
  /// Text has been entered.
  EnteredText(String),
  /// A indication that some component changed and that we should
  /// re-render everything.
  Updated,
  /// Retrieve the current set of tasks.
  #[cfg(test)]
  GetTasks,
  /// The response to the `GetTasks` event.
  #[cfg(test)]
  GetTasksResp(Vec<Task>),
  /// Retrieve the current state of the input/output area.
  #[cfg(test)]
  GetInOut,
  /// The response to the `GetInOut` event.
  #[cfg(test)]
  GetInOutResp(InOut),
}

impl TermUiEvent {
  /// Check whether the event is the `Updated` variant.
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
  tab_bar: Id,
  state: State,
}


impl TermUi {
  /// Create a new view associated with the given `State` object.
  pub fn new(id: Id, cap: &mut Cap, state: State) -> Result<Self> {
    let in_out = cap.add_widget(id, &mut |id, cap| {
      Box::new(InOutArea::new(id, cap))
    });
    let tab_bar = cap.add_widget(id, &mut |id, cap| {
      Box::new(TabBar::new(id, cap, &state))
    });

    Ok(TermUi {
      id: id,
      in_out: in_out,
      tab_bar: tab_bar,
      state: state,
    })
  }

  /// Save the current state.
  fn save(&mut self) -> MetaEvent {
    let in_out = match self.state.save() {
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
        let id = self.state.add_task(s);
        // We have no knowledge of which `TaskListBox` widget is
        // currently active. So send the response to the `TabBar` to
        // forward it accordingly.
        let resp = TermUiEvent::AddTaskResp(id);
        let event = UiEvent::Custom(self.tab_bar, Box::new(resp));
        Some(MetaEvent::UiEvent(event)).update()
      },
      TermUiEvent::RemoveTask(id) => {
        self.state.remove_task(id);
        (None as Option<Event>).update()
      },
      TermUiEvent::UpdateTask(task) => {
        self.state.update_task(task);
        (None as Option<Event>).update()
      },
      TermUiEvent::SetInOut(_) => {
        Some(UiEvent::Custom(self.in_out, event).into())
      },
      #[cfg(test)]
      TermUiEvent::GetTasks => {
        let tasks = self.state.tasks();
        let resp = TermUiEvent::GetTasksResp(tasks);
        Some(Event::Custom(Box::new(resp)).into())
      },
      #[cfg(test)]
      TermUiEvent::GetInOut => {
        // We merely relay this event to the InOutArea widget, which is
        // the only entity able to satisfy the request.
        Some(UiEvent::Custom(self.in_out, event).into())
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

  use event::tests::CustomEvent;
  use tasks::Tasks;
  use tasks::tests::make_tasks_vec;
  use tasks::tests::NamedTempFile;
  use tasks::tests::TaskVec;


  fn test_ui(task_vec: TaskVec, mut events: Vec<UiEvent>) -> Ui {
    let mut tasks = Some(Tasks::from(task_vec));
    let file = NamedTempFile::new();
    let (mut ui, _) = Ui::new(&mut |id, cap| {
      let tasks = tasks.take().unwrap();
      let state = State::with_tasks_and_path(tasks, file.path());
      Box::new(TermUi::new(id, cap, state).unwrap())
    });

    for event in events.drain(..) {
      if let Some(event) = ui.handle(event) {
        if let UiEvent::Quit = event.into_last() {
          break
        }
      }
    }
    ui
  }

  /// Test function for the TermUi.
  ///
  /// Instantiate the TermUi in a mock environment associating it with
  /// the given task list, supply the given input, and retrieve the
  /// resulting set of tasks.
  fn test(task_vec: TaskVec, events: Vec<UiEvent>) -> TaskVec {
    let mut ui = test_ui(task_vec, events);
    let event = Event::Custom(Box::new(TermUiEvent::GetTasks));
    let resp = ui.handle(event).unwrap().unwrap_custom::<TermUiEvent>();
    let tasks = if let TermUiEvent::GetTasksResp(tasks) = resp {
      tasks
    } else {
      panic!("Unexpected response: {:?}", resp)
    };
    TaskVec(tasks)
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
  fn add_task_with_cursor_movement() {
    let tasks = make_tasks_vec(1);
    let events = vec![
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Delete).into(),
      Event::KeyDown(Key::Right).into(),
      Event::KeyDown(Key::Char('s')).into(),
      Event::KeyDown(Key::Char('t')).into(),
      Event::KeyDown(Key::Home).into(),
      Event::KeyDown(Key::Home).into(),
      Event::KeyDown(Key::Char('t')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::End).into(),
      Event::KeyDown(Key::End).into(),
      Event::KeyDown(Key::Delete).into(),
      Event::KeyDown(Key::Char('4')).into(),
      Event::KeyDown(Key::Char('2')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];
    let mut expected = make_tasks_vec(1);
    expected.push(Task::new("test42"));
    assert_eq!(test(tasks, events), expected)
  }

  #[test]
  fn add_empty_task() {
    let tasks = make_tasks_vec(1);
    let events = vec![
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];

    let expected = make_tasks_vec(1);
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

  #[test]
  fn edit_task() {
    let tasks = make_tasks_vec(3);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Backspace).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('m')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Char('n')).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];

    let tasks = test(tasks, events);
    assert_eq!(tasks[1].summary, "amend".to_string());
  }

  #[test]
  fn edit_task_with_cursor_movement() {
    let tasks = make_tasks_vec(3);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Left).into(),
      Event::KeyDown(Key::Left).into(),
      Event::KeyDown(Key::Left).into(),
      Event::KeyDown(Key::Delete).into(),
      Event::KeyDown(Key::Char('t')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Left).into(),
      Event::KeyDown(Key::Delete).into(),
      Event::KeyDown(Key::Right).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Char('s')).into(),
      Event::KeyDown(Key::Char('t')).into(),
      Event::KeyDown(Key::Right).into(),
      Event::KeyDown(Key::Right).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('q')).into(),
    ];

    let tasks = test(tasks, events);
    assert_eq!(tasks[2].summary, "test".to_string());
  }

  /// Test function for the `TermUi` that returns the state of the `InOutArea` widget.
  fn test_state(task_vec: TaskVec, events: Vec<UiEvent>) -> InOut {
    let mut ui = test_ui(task_vec, events);
    let event = Event::Custom(Box::new(TermUiEvent::GetInOut));
    let resp = ui.handle(event).unwrap().unwrap_custom::<TermUiEvent>();

    if let TermUiEvent::GetInOutResp(in_out) = resp {
      in_out
    } else {
      panic!("Unexpected response: {:?}", resp)
    }
  }

  #[test]
  fn in_out_state_after_write() {
    let tasks = make_tasks_vec(2);
    let events = vec![
      Event::KeyDown(Key::Char('w')).into(),
    ];

    assert_eq!(test_state(tasks, events), InOut::Saved);
  }

  #[test]
  fn in_out_state_after_write_and_key_press() {
    fn with_key(key: Key) -> InOut {
      let tasks = make_tasks_vec(2);
      let events = vec![
        Event::KeyDown(Key::Char('w')).into(),
        Event::KeyDown(key).into(),
      ];

      test_state(tasks, events)
    }

    // We test all ASCII chars.
    for c in 0u8..127u8 {
      let c = c as char;
      if c != 'a' && c != 'e' && c != 'w' {
        assert_eq!(with_key(Key::Char(c as char)), InOut::Clear, "char: {} ({})", c, c as u8);
      }
    }

    assert_eq!(with_key(Key::Esc), InOut::Clear);
    assert_eq!(with_key(Key::Return), InOut::Clear);
    assert_eq!(with_key(Key::PageDown), InOut::Clear);
  }

  #[test]
  fn in_out_state_on_edit() {
    fn with_key(key: Key) -> InOut {
      let tasks = make_tasks_vec(4);
      let events = vec![
        Event::KeyDown(Key::Char('a')).into(),
        Event::KeyDown(key).into(),
      ];

      test_state(tasks, events)
    }

    for c in 0u8..127u8 {
      let state = with_key(Key::Char(c as char));
      match state {
        InOut::Input(_, _) => (),
        _ => assert!(false, "Unexpected state {:?} for char {}", state, c),
      }
    }

    assert_eq!(with_key(Key::Esc), InOut::Clear);
    assert_eq!(with_key(Key::Return), InOut::Clear);
  }
}
