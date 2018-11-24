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
use gui::UiEvent;
use gui::UiEvents;

use crate::in_out::InOut;
use crate::in_out::InOutArea;
use crate::state::State;
use crate::tab_bar::SearchState;
use crate::tab_bar::SelectionState;
use crate::tab_bar::TabBar;
use crate::tasks::Id as TaskId;
#[cfg(all(test, not(feature = "readline")))]
use crate::tasks::Task;


/// An enumeration comprising all custom events we support.
#[derive(Debug)]
pub enum TermUiEvent {
  /// An event to ask a widget to select the task with the given
  /// `TaskId`.
  SelectTask(TaskId, SelectionState),
  /// Search for a task containing the given string in its summary and
  /// select it.
  SearchTask(String, SearchState, SelectionState),
  /// The tab with the given `Id` has selected the task as indicated by
  /// `SelectTask` or `SearchTask`.
  SelectedTask(Id),
  /// Set the state of the input/output area.
  SetInOut(InOut),
  /// Change the state of the input/output area to Clear, unless the
  /// generation ID supplied does not match the current generation ID.
  /// This event is internal to the InOutArea, there is no need for
  /// other clients to use it.
  ClearInOut(usize),
  /// Text has been entered.
  EnteredText(String),
  /// Text input has been canceled.
  InputCanceled,
  /// A indication that some component changed and that we should
  /// re-render everything.
  Updated,
  /// Retrieve the current set of tasks.
  #[cfg(all(test, not(feature = "readline")))]
  GetTasks,
  /// The response to the `GetTasks` event.
  #[cfg(all(test, not(feature = "readline")))]
  GetTasksResp(Vec<Task>),
  /// Retrieve the current state of the input/output area.
  #[cfg(all(test, not(feature = "readline")))]
  GetInOut,
  /// The response to the `GetInOut` event.
  #[cfg(all(test, not(feature = "readline")))]
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
  pub fn new(id: Id, cap: &mut dyn Cap, state: State) -> Result<Self> {
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
  fn save(&mut self) -> UiEvents {
    let in_out = match self.state.save() {
      Ok(_) => InOut::Saved,
      Err(err) => InOut::Error(format!("{}", err)),
    };
    let event = TermUiEvent::SetInOut(in_out);
    UiEvent::Directed(self.in_out, Box::new(event)).into()
  }

  /// Handle a custom event.
  fn handle_custom_event(&mut self, event: Box<TermUiEvent>) -> Option<UiEvents> {
    match *event {
      TermUiEvent::SetInOut(_) => {
        Some(UiEvent::Directed(self.in_out, event).into())
      },
      #[cfg(all(test, not(feature = "readline")))]
      TermUiEvent::GetTasks => {
        let tasks = self.state.tasks();
        let tasks = tasks.borrow().iter().cloned().collect();
        let resp = TermUiEvent::GetTasksResp(tasks);
        Some(UiEvent::Custom(Box::new(resp)).into())
      },
      #[cfg(all(test, not(feature = "readline")))]
      TermUiEvent::GetInOut => {
        // We merely relay this event to the InOutArea widget, which is
        // the only entity able to satisfy the request.
        Some(UiEvent::Directed(self.in_out, event).into())
      },
      _ => Some(UiEvent::Custom(event).into()),
    }
  }
}

impl Handleable for TermUi {
  /// Check for new input and react to it.
  fn handle(&mut self, event: Event, _cap: &mut dyn Cap) -> Option<UiEvents> {
    match event {
      Event::KeyDown(key) |
      Event::KeyUp(key) => {
        match key {
          Key::Char('q') => Some(UiEvent::Quit.into()),
          Key::Char('w') => Some(self.save()),
          _ => Some(event.into()),
        }
      },
    }
  }

  /// Handle a custom event.
  fn handle_custom(&mut self, event: Box<dyn Any>, _cap: &mut dyn Cap) -> Option<UiEvents> {
    match event.downcast::<TermUiEvent>() {
      Ok(e) => self.handle_custom_event(e),
      Err(e) => panic!("Received unexpected custom event: {:?}", e),
    }
  }
}


// We can't run the "end-to-end" tests in conjunction with readline
// support. Readline can be configured outside of this program's control
// and so key bindings could be arbitrary.
#[cfg(all(test, not(feature = "readline")))]
mod tests {
  use super::*;

  use gui::Ui;
  use gui::UnhandledEvent;
  use gui::UnhandledEvents;

  use crate::event::EventUpdated;
  use crate::event::tests::CustomEvent;
  use crate::ser::query::Query as SerQuery;
  use crate::ser::query::TagLit as SerTagLit;
  use crate::ser::state::ProgState as SerProgState;
  use crate::ser::state::TaskState as SerTaskState;
  use crate::ser::tags::Templates as SerTemplates;
  use crate::ser::tasks::Task as SerTask;
  use crate::ser::tasks::Tasks as SerTasks;
  use crate::ser::ToSerde;
  use crate::test::make_tasks;
  use crate::test::make_tasks_with_tags;
  use crate::test::NamedTempFile;


  /// A builder object used for instantiating a UI with a certain
  /// composition of tasks.
  struct TestUiBuilder {
    prog_state: SerProgState,
    task_state: SerTaskState,
  }

  impl TestUiBuilder {
    /// Create a builder that will create a UI without any tasks.
    fn new() -> TestUiBuilder {
      TestUiBuilder {
        prog_state: Default::default(),
        task_state: Default::default(),
      }
    }

    /// Create a builder that will create a UI with the given tasks.
    fn with_ser_tasks<T>(tasks: T) -> TestUiBuilder
    where
      T: AsRef<[SerTask]> + Into<Vec<SerTask>>,
    {
      tasks.as_ref().iter().for_each(|x| assert!(x.tags.is_empty()));

      TestUiBuilder {
        prog_state: Default::default(),
        task_state: SerTaskState {
          templates: Default::default(),
          tasks: SerTasks(tasks.into()),
        },
      }
    }

    /// Create a builder that will instantiate a UI with four queries
    /// and 15 tasks with tags. Tag assignment follows the pattern that
    /// `make_tasks_with_tags` creates.
    fn with_default_tasks_and_tags() -> TestUiBuilder {
      let (tags, templates, tasks) = make_tasks_with_tags(15);
      let prog_state = SerProgState {
        queries: vec![
          SerQuery {
            name: "all".to_string(),
            lits: vec![],
          },
          SerQuery {
            name: "tag complete".to_string(),
            lits: vec![vec![SerTagLit::Pos(tags[0])]],
          },
          SerQuery {
            name: "tag2 || tag3".to_string(),
            lits: vec![
              vec![
                SerTagLit::Pos(tags[2]),
                SerTagLit::Pos(tags[3]),
              ],
            ],
          },
          SerQuery {
            name: "tag1 && tag3".to_string(),
            lits: vec![
              vec![SerTagLit::Pos(tags[1])],
              vec![SerTagLit::Pos(tags[3])],
            ],
          },
        ],
      };
      let task_state = SerTaskState {
        templates: SerTemplates(templates),
        tasks: SerTasks(tasks),
      };

      TestUiBuilder {
        prog_state: prog_state,
        task_state: task_state,
      }
    }

    /// Build the actual UI object that we can test with.
    fn build(self) -> TestUi {
      let mut prog_state = Some(self.prog_state);
      let mut task_state = Some(self.task_state);
      let prog_file = NamedTempFile::new();
      let task_file = NamedTempFile::new();
      let (ui, _) = Ui::new(&mut |id, cap| {
        let prog_state = prog_state.take().unwrap();
        let task_state = task_state.take().unwrap();
        let state = State::with_serde(prog_state, prog_file.path(), task_state, task_file.path());
        Box::new(TermUi::new(id, cap, state.unwrap()).unwrap())
      });

      TestUi {
        prog_file: prog_file,
        task_file: task_file,
        ui: ui,
      }
    }
  }

  /// An UI object used for testing. It is just a handy wrapper around a
  /// `Ui`.
  #[allow(unused)]
  struct TestUi {
    prog_file: NamedTempFile,
    task_file: NamedTempFile,
    ui: Ui,
  }

  impl TestUi {
    /// Handle a single event and directly return the result.
    fn evaluate<E>(&mut self, event: E) -> Option<UnhandledEvents>
    where
      E: Into<UiEvent>,
    {
      self.ui.handle(event)
    }

    /// Send the given list of events to the UI.
    fn handle(&mut self, mut events: Vec<UiEvent>) -> &mut Self {
      for event in events.drain(..) {
        if let Some(event) = self.ui.handle(event) {
          if let UnhandledEvent::Quit = event.into_last() {
            break
          }
        }
      }
      self
    }

    /// Retrieve the current `InOutArea` state.
    fn in_out(&mut self) -> InOut {
      let event = UiEvent::Custom(Box::new(TermUiEvent::GetInOut));
      let resp = self
        .ui
        .handle(event)
        .unwrap()
        .unwrap_custom::<TermUiEvent>();

      if let TermUiEvent::GetInOutResp(in_out) = resp {
        in_out
      } else {
        panic!("Unexpected response: {:?}", resp)
      }
    }

    /// Retrieve the current set of tasks from the UI.
    fn tasks(&mut self) -> Vec<Task> {
      let event = UiEvent::Custom(Box::new(TermUiEvent::GetTasks));
      let resp = self
        .ui
        .handle(event)
        .unwrap()
        .unwrap_custom::<TermUiEvent>();

      if let TermUiEvent::GetTasksResp(tasks) = resp {
        tasks
      } else {
        panic!("Unexpected response: {:?}", resp)
      }
    }

    /// Retrieve the current set of tasks in the form of `SerTask` objects from the UI.
    fn ser_tasks(&mut self) -> Vec<SerTask> {
      self.tasks().iter().map(|x| x.to_serde()).collect()
    }
  }

  #[test]
  fn exit_on_quit() {
    let events = vec![
      Event::KeyDown(Key::Char('q')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('f')).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::new()
      .build()
      .handle(events)
      .ser_tasks();

    assert_eq!(tasks, make_tasks(0))
  }

  #[test]
  fn remove_no_task() {
    let events = vec![
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let tasks = TestUiBuilder::new()
      .build()
      .handle(events)
      .ser_tasks();

    assert_eq!(tasks, make_tasks(0))
  }

  #[test]
  fn remove_only_task() {
    let tasks = make_tasks(1);
    let events = vec![
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    assert_eq!(tasks, make_tasks(0))
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
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    assert_eq!(tasks, make_tasks(1))
  }

  #[test]
  fn remove_task_after_up_select() {
    let tasks = make_tasks(3);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(3);
    expected.remove(1);
    assert_eq!(tasks, expected)
  }

  #[test]
  fn remove_last_task() {
    let tasks = make_tasks(3);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(3);
    expected.remove(0);
    expected.remove(1);
    assert_eq!(tasks, expected)
  }

  #[test]
  fn remove_second_to_last_task() {
    let tasks = make_tasks(5);
    let events = vec![
      Event::KeyDown(Key::Char('G')).into(),
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(5);
    expected.remove(3);
    assert_eq!(tasks, expected)
  }

  #[test]
  fn remove_second_task_after_up_select() {
    let tasks = make_tasks(3);
    let events = vec![
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('k')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(3);
    expected.remove(1);
    assert_eq!(tasks, expected)
  }

  #[test]
  fn selection_after_removal() {
    let tasks = make_tasks(5);
    let events = vec![
      Event::KeyDown(Key::Char('G')).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Backspace).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(4);
    expected[3].summary = "o".to_string();
    assert_eq!(tasks, expected)
  }

  #[test]
  fn add_task() {
    let events = vec![
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('f')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Char('b')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('r')).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::new()
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(1);
    expected[0].summary = "foobar".to_string();

    assert_eq!(tasks, expected)
  }

  #[test]
  fn add_and_remove_tasks() {
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
    ];

    let tasks = TestUiBuilder::new()
      .build()
      .handle(events)
      .ser_tasks();

    assert_eq!(tasks, make_tasks(0))
  }

  #[test]
  fn add_task_cancel() {
    let events = vec![
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('f')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Char('b')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('z')).into(),
      Event::KeyDown(Key::Esc).into(),
    ];

    let tasks = TestUiBuilder::new()
      .build()
      .handle(events)
      .ser_tasks();

    assert_eq!(tasks, make_tasks(0))
  }

  #[test]
  fn add_task_with_character_removal() {
    let tasks = make_tasks(1);
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
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(2);
    expected[1].summary = "baz".to_string();

    assert_eq!(tasks, expected)
  }

  #[test]
  fn add_task_with_cursor_movement() {
    let tasks = make_tasks(1);
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
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(2);
    expected[1].summary = "test42".to_string();

    assert_eq!(tasks, expected)
  }

  #[test]
  fn add_empty_task() {
    let tasks = make_tasks(1);
    let events = vec![
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    assert_eq!(tasks, make_tasks(1))
  }

  #[test]
  fn add_task_from_completed_one() {
    let events = vec![
      Event::KeyDown(Key::Char('l')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('t')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Char('s')).into(),
      Event::KeyDown(Key::Char('t')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('l')).into(),
      Event::KeyDown(Key::Char('l')).into(),
      Event::KeyDown(Key::Char('g')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('h')).into(),
      Event::KeyDown(Key::Char('i')).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .tasks();

    assert_eq!(tasks[15].summary, "test");

    let tags = tasks[15]
      .tags()
      .map(|x| x.name())
      .collect::<Vec<_>>();
    let expected = Vec::<String>::new();
    assert_eq!(tags, expected);

    assert_eq!(tasks[16].summary, "hi");

    let tags = tasks[16]
      .tags()
      .map(|x| x.name())
      .collect::<Vec<_>>();
    let expected = vec!["tag2"];
    assert_eq!(tags, expected);
  }

  #[test]
  fn add_task_with_tags() {
    let events = vec![
      Event::KeyDown(Key::Char('l')).into(),
      Event::KeyDown(Key::Char('l')).into(),
      Event::KeyDown(Key::Char('l')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('f')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .tasks();

    assert_eq!(tasks[15].summary, "foo");

    let tags = tasks[15]
      .tags()
      .map(|x| x.name())
      .collect::<Vec<_>>();
    let expected = vec![
      "tag1".to_string(),
      "tag2".to_string(),
      "tag3".to_string(),
    ];
    assert_eq!(tags, expected);
  }

  #[test]
  fn complete_task() {
    let tasks = make_tasks(3);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char(' ')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char(' ')).into(),
      Event::KeyDown(Key::Char(' ')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .tasks();

    assert!(!tasks[0].is_complete());
    assert!(tasks[1].is_complete());
    assert!(!tasks[2].is_complete());
  }

  #[test]
  fn edit_task() {
    let tasks = make_tasks(3);
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
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(3);
    expected[1].summary = "amend".to_string();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn edit_task_cancel() {
    let tasks = make_tasks(3);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Esc).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('f')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Char('o')).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(4);
    expected[3].summary = "foo".to_string();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn edit_task_to_empty() {
    let tasks = make_tasks(3);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Backspace).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(3);
    expected.remove(1);

    assert_eq!(tasks, expected)
  }

  #[test]
  fn edit_task_multi_byte_char() {
    let tasks = make_tasks(1);
    let events = vec![
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Char('ä')).into(),
      Event::KeyDown(Key::Char('ö')).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    assert_eq!(tasks, make_tasks(1))
  }

  #[test]
  fn edit_task_with_cursor_movement() {
    let tasks = make_tasks(3);
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
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(3);
    expected[2].summary = "test".to_string();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn edit_multiple_tasks() {
    let events = vec![
      Event::KeyDown(Key::Char('3')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Char('b')).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .tasks()
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    let (.., mut expected) = make_tasks_with_tags(15);
    expected[8].summary = "9ab".to_string();
    let expected = expected.drain(..).map(|x| x.summary).collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn move_no_or_one_task_up_or_down() {
    fn test_tasks(count: usize, key: char) {
      let tasks = make_tasks(count);
      let events = vec![
        Event::KeyDown(Key::Char(key)).into(),
      ];

      let tasks = TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .ser_tasks();

      assert_eq!(tasks, make_tasks(count));
    }

    test_tasks(0, 'J');
    test_tasks(1, 'J');
    test_tasks(0, 'K');
    test_tasks(1, 'K');
  }

  #[test]
  fn move_task_down() {
    let tasks = make_tasks(3);
    let events = vec![
      Event::KeyDown(Key::Char('J')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(3);
    expected.swap(0, 1);

    assert_eq!(tasks, expected);
  }

  #[test]
  fn move_task_up() {
    let tasks = make_tasks(3);
    let events = vec![
      Event::KeyDown(Key::Char('G')).into(),
      Event::KeyDown(Key::Char('K')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(3);
    expected.swap(2, 1);

    assert_eq!(tasks, expected);
  }

  #[test]
  fn move_second_task_down() {
    let tasks = make_tasks(4);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('J')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(4);
    expected.swap(1, 2);

    assert_eq!(tasks, expected);
  }

  #[test]
  fn move_second_task_up() {
    let tasks = make_tasks(4);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('K')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(4);
    expected.swap(1, 0);

    assert_eq!(tasks, expected);
  }

  #[test]
  fn transparent_task_removal_down_to_empty_query() {
    let events = vec![
      Event::KeyDown(Key::Char('l')).into(),
      Event::KeyDown(Key::Char('l')).into(),
      Event::KeyDown(Key::Char('l')).into(),
      Event::KeyDown(Key::Char('G')).into(),
      Event::KeyDown(Key::Char('h')).into(),
      Event::KeyDown(Key::Char('h')).into(),
      Event::KeyDown(Key::Char('h')).into(),
      Event::KeyDown(Key::Char('G')).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('l')).into(),
      Event::KeyDown(Key::Char('l')).into(),
      Event::KeyDown(Key::Char('l')).into(),
      // The edit should be a no-op.
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .tasks();

    assert_eq!(tasks.len(), 14);
  }

  #[test]
  fn tab_selection_by_number() {
    let events = vec![
      Event::KeyDown(Key::Char('4')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .tasks()
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    let (.., mut expected) = make_tasks_with_tags(15);
    expected[14].summary = "15a".to_string();
    let expected = expected
      .drain(..)
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn select_previous_tab() {
    let events = vec![
      Event::KeyDown(Key::Char('2')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('4')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('`')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Return).into(),
    ];

    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .tasks()
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    let (.., mut expected) = make_tasks_with_tags(15);
    expected[1].summary = "2a".to_string();
    expected[3].summary = "4a".to_string();
    expected[14].summary = "15a".to_string();
    let expected = expected
      .drain(..)
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn select_last_tab_plus_one() {
    let events = vec![
      Event::KeyDown(Key::Char('0')).into(),
      Event::KeyDown(Key::Char('l')).into(),
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .tasks()
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    let (.., mut expected) = make_tasks_with_tags(15);
    expected.remove(14);
    let expected = expected
      .drain(..)
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn in_out_state_after_write() {
    let tasks = make_tasks(2);
    let events = vec![
      Event::KeyDown(Key::Char('w')).into(),
    ];

    let state = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .in_out();

    assert_eq!(state, InOut::Saved);
  }

  #[test]
  fn in_out_state_after_write_and_key_press() {
    fn with_key(key: Key) -> InOut {
      let tasks = make_tasks(2);
      let events = vec![
        Event::KeyDown(Key::Char('w')).into(),
        Event::KeyDown(key).into(),
      ];

      TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .in_out()
    }

    // We test all ASCII chars.
    for c in 0u8..127u8 {
      let c = c as char;
      if c != 'a' && c != 'e' && c != 'n' && c != 'N' && c != 'w' && c != '/' && c != '?' {
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
      let tasks = make_tasks(4);
      let events = vec![
        Event::KeyDown(Key::Char('a')).into(),
        Event::KeyDown(key).into(),
      ];

      TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .in_out()
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

  #[test]
  fn search_abort() {
    fn test(c: char) {
      let tasks = make_tasks(4);
      let events = vec![
        Event::KeyDown(Key::Char(c)).into(),
        Event::KeyDown(Key::Char('f')).into(),
        Event::KeyDown(Key::Esc).into(),
      ];

      let state = TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .in_out();

      assert_eq!(state, InOut::Clear);
    }

    test('/');
    test('?');
  }

  #[test]
  fn search_tasks_not_found() {
    fn test(c: char) {
      let tasks = make_tasks(8);
      let events = vec![
        Event::KeyDown(Key::Char(c)).into(),
        Event::KeyDown(Key::Char('f')).into(),
        Event::KeyDown(Key::Char('o')).into(),
        Event::KeyDown(Key::Char('o')).into(),
        Event::KeyDown(Key::Return).into(),
      ];

      let state = TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .in_out();

      let expected = InOut::Error("Text 'foo' not found".to_string());
      assert_eq!(state, expected);
    }

    test('/');
    test('?');
  }

  #[test]
  fn search_multiple_tabs_not_found() {
    fn test(c: char) {
      let events = vec![
        Event::KeyDown(Key::Char(c)).into(),
        Event::KeyDown(Key::Char('f')).into(),
        Event::KeyDown(Key::Char('o')).into(),
        Event::KeyDown(Key::Char('o')).into(),
        Event::KeyDown(Key::Return).into(),
      ];

      let state = TestUiBuilder::with_default_tasks_and_tags()
        .build()
        .handle(events)
        .in_out();

      let expected = InOut::Error("Text 'foo' not found".to_string());
      assert_eq!(state, expected);
    }

    test('/');
    test('?');
  }

  #[test]
  fn search_continue_without_start() {
    fn test(c: char) {
      let tasks = make_tasks(4);
      let events = vec![
        Event::KeyDown(Key::Char(c)).into(),
        Event::KeyDown(Key::Char(c)).into(),
        Event::KeyDown(Key::Char(c)).into(),
      ];

      let state = TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .in_out();

      let expected = InOut::Error("Nothing to search for".to_string());
      assert_eq!(state, expected);
    }

    test('n');
    test('N');
  }

  #[test]
  fn search_tasks_repeatedly_not_found() {
    fn test(c: char) {
      let tasks = make_tasks(8);
      let events = vec![
        Event::KeyDown(Key::Char('/')).into(),
        Event::KeyDown(Key::Char('z')).into(),
        Event::KeyDown(Key::Return).into(),
        Event::KeyDown(Key::Char(c)).into(),
      ];

      let state = TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .in_out();

      let expected = InOut::Error("Text 'z' not found".to_string());
      assert_eq!(state, expected);
    }

    test('n');
    test('N');
  }

  #[test]
  fn search_tasks() {
    let tasks = make_tasks(12);
    let events = vec![
      Event::KeyDown(Key::Char('/')).into(),
      Event::KeyDown(Key::Char('2')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('d')).into(),
      Event::KeyDown(Key::Char('n')).into(),
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(12);
    expected.remove(11);
    expected.remove(1);

    assert_eq!(tasks, expected);
  }

  #[test]
  fn search_reverse() {
    let tasks = make_tasks(12);
    let events = vec![
      Event::KeyDown(Key::Char('?')).into(),
      Event::KeyDown(Key::Char('2')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(12);
    expected.remove(11);

    assert_eq!(tasks, expected);
  }

  #[test]
  fn search_case_insensitive() {
    fn test(c: char) {
      let tasks = vec![
        SerTask {
          summary: "First".to_string(),
          tags: Default::default(),
        },
        SerTask {
          summary: "SeCOnd".to_string(),
          tags: Default::default(),
        },
      ];
      let events = vec![
        Event::KeyDown(Key::Char(c)).into(),
        Event::KeyDown(Key::Char('c')).into(),
        Event::KeyDown(Key::Char('o')).into(),
        Event::KeyDown(Key::Char('N')).into(),
        Event::KeyDown(Key::Char('d')).into(),
        Event::KeyDown(Key::Return).into(),
        Event::KeyDown(Key::Char('d')).into(),
      ];

      let tasks = TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .ser_tasks();

      let expected = vec![
        SerTask {
          summary: "First".to_string(),
          tags: Default::default(),
        },
      ];
      assert_eq!(tasks, expected);
    }

    test('/');
    test('?');
  }

  #[test]
  fn search_starting_at_selection() {
    let tasks = make_tasks(15);
    let events = vec![
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('j')).into(),
      Event::KeyDown(Key::Char('/')).into(),
      Event::KeyDown(Key::Char('1')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(15);
    expected.remove(9);

    assert_eq!(tasks, expected);
  }

  #[test]
  fn search_overlapping() {
    let tasks = make_tasks(5);
    let events = vec![
      Event::KeyDown(Key::Char('G')).into(),
      Event::KeyDown(Key::Char('/')).into(),
      Event::KeyDown(Key::Char('2')).into(),
      Event::KeyDown(Key::Return).into(),
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(5);
    expected.remove(1);

    assert_eq!(tasks, expected);
  }

  #[test]
  fn search_tasks_on_multiple_tabs() {
    fn test(c1: char, c2: char) {
      let events = vec![
        // Switch to tag2 || tag3 tab.
        Event::KeyDown(Key::Char('l')).into(),
        // Search for a task with '4' in it.
        Event::KeyDown(Key::Char(c1)).into(),
        Event::KeyDown(Key::Char('4')).into(),
        Event::KeyDown(Key::Return).into(),
        // Delete it. That should be task 14.
        Event::KeyDown(Key::Char('d')).into(),
        // Move to next task.
        Event::KeyDown(Key::Char(c2)).into(),
        // Delete it. That should be task 4.
        Event::KeyDown(Key::Char('d')).into(),
      ];

      let tasks = TestUiBuilder::with_default_tasks_and_tags()
        .build()
        .handle(events)
        .tasks()
        .into_iter()
        .map(|x| x.summary)
        .collect::<Vec<_>>();

      let (.., mut expected) = make_tasks_with_tags(15);
      expected.remove(13);
      expected.remove(3);
      let expected = expected.drain(..).map(|x| x.summary).collect::<Vec<_>>();

      assert_eq!(tasks, expected);
    }

    test('/', 'n');
    test('/', 'N');
    test('?', 'n');
    test('?', 'N');
  }

  #[test]
  fn search_wrap_around() {
    let events = vec![
      Event::KeyDown(Key::Char('/')).into(),
      Event::KeyDown(Key::Char('2')).into(),
      // After this event we should have selected task '2'.
      Event::KeyDown(Key::Return).into(),
      // After this event task '12'.
      Event::KeyDown(Key::Char('n')).into(),
      // Rename.
      Event::KeyDown(Key::Char('e')).into(),
      Event::KeyDown(Key::Backspace).into(),
      Event::KeyDown(Key::Backspace).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Char('a')).into(),
      Event::KeyDown(Key::Return).into(),
      // '2'
      Event::KeyDown(Key::Char('n')).into(),
      // '2'
      Event::KeyDown(Key::Char('n')).into(),
      Event::KeyDown(Key::Char('d')).into(),
    ];

    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .tasks()
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    let (.., mut expected) = make_tasks_with_tags(15);
    expected[11].summary = "aa".to_string();
    expected.remove(1);
    let expected = expected.drain(..).map(|x| x.summary).collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn valid_update_events() {
    for c in 0u8..127u8 {
      let c = c as char;
      let mut ui = TestUiBuilder::new().build();
      let updated = ui
        .evaluate(Event::KeyDown(Key::Char(c as char)))
        .map_or(false, |x| x.is_updated());

      let expected = c == '/' || c == '?' || c == 'a' || c == 'n' || c == 'N' || c == 'w';
      assert_eq!(updated, expected, "char: {} ({})", c, c as u8);
    }
  }

  #[test]
  fn search_no_update_without_change() {
    let mut ui = TestUiBuilder::new().build();
    let updated = ui
      .evaluate(Event::KeyDown(Key::Char('n')))
      .map_or(false, |x| x.is_updated());

    assert!(updated);

    let updated = ui
      .evaluate(Event::KeyDown(Key::Char('n')))
      .map_or(false, |x| x.is_updated());

    assert!(!updated);
  }
}
