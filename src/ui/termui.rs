// termui.rs

// *************************************************************************
// * Copyright (C) 2017-2020 Daniel Mueller (deso@posteo.net)              *
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
use std::path::PathBuf;

use gui::derive::Widget;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::UiEvent;
use gui::UiEvents;
use gui::Widget;

use crate::state::TaskState;
use crate::state::UiState;
#[cfg(all(test, not(feature = "readline")))]
use crate::tasks::Task;

use super::event::Event;
use super::event::Key;
use super::in_out::InOut;
use super::in_out::InOutArea;
use super::in_out::InOutAreaData;
use super::message::Message;
use super::tab_bar::TabBar;
use super::tab_bar::TabBarData;
use super::tab_bar::TabState;


/// The data associated with a `TermUi`.
pub struct TermUiData {
  task_state: TaskState,
  ui_state_path: PathBuf,
}

impl TermUiData {
  pub fn new(task_state: TaskState, ui_state_path: PathBuf) -> Self {
    Self {
      task_state,
      ui_state_path,
    }
  }
}

/// An implementation of a terminal based view.
#[derive(Debug, Widget)]
#[gui(Event = Event)]
pub struct TermUi {
  id: Id,
  in_out: Id,
  tab_bar: Id,
}


impl TermUi {
  /// Create a new view associated with the given `State` object.
  pub fn new(id: Id, cap: &mut dyn MutCap<Event>, state: UiState) -> Self {
    let termui_id = id;
    let UiState {
      queries, selected, ..
    } = state;

    let in_out = cap.add_widget(
      id,
      Box::new(|| Box::new(InOutAreaData::new())),
      Box::new(|id, cap| Box::new(InOutArea::new(id, cap))),
    );
    let tab_bar = cap.add_widget(
      id,
      Box::new(|| Box::new(TabBarData::new())),
      Box::new(move |id, cap| {
        let data = cap.data(termui_id).downcast_ref::<TermUiData>().unwrap();
        let tasks = data.task_state.tasks();
        Box::new(TabBar::new(id, cap, tasks, queries, selected))
      }),
    );

    Self {
      id,
      in_out,
      tab_bar,
    }
  }

  /// Persist the state into a file.
  fn save_all(&self, task_state: &TaskState, ui_state: &UiState) -> Result<()> {
    task_state.save()?;
    // TODO: We risk data inconsistencies if the second save operation
    //       fails.
    ui_state.save()?;
    Ok(())
  }

  /// Save the current state.
  fn save_and_report(&self, task_state: &TaskState, ui_state: &UiState) -> UiEvents<Event> {
    let in_out = match self.save_all(task_state, ui_state) {
      Ok(_) => InOut::Saved,
      Err(err) => InOut::Error(format!("{}", err)),
    };
    let event = Message::SetInOut(in_out);
    UiEvent::Directed(self.in_out, Box::new(event)).into()
  }

  /// Emit an event that will eventually cause the state to be saved.
  fn save(&self) -> UiEvents<Event> {
    let event = Message::CollectState(true);
    UiEvent::Directed(self.tab_bar, Box::new(event)).into()
  }

  /// Handle a custom event.
  fn handle_custom_event(
    &self,
    event: Box<Message>,
    cap: &mut dyn MutCap<Event>,
  ) -> Option<UiEvents<Event>> {
    let data = self.data_mut::<TermUiData>(cap);

    match *event {
      Message::CollectedState(state) if state.for_save => {
        let TabState{queries, selected, ..} = state;
        let ui_state = UiState {
          path: data.ui_state_path.clone(),
          queries,
          selected,
          colors: Default::default(),
        };
        Some(self.save_and_report(&data.task_state, &ui_state))
      },
      Message::SetInOut(_) => {
        Some(UiEvent::Directed(self.in_out, event).into())
      },
      #[cfg(all(test, not(feature = "readline")))]
      Message::GetTasks => {
        let tasks = data.task_state.tasks();
        let tasks = tasks.borrow().iter().cloned().collect();
        let resp = Message::GotTasks(tasks);
        Some(UiEvent::Custom(Box::new(resp)).into())
      },
      #[cfg(all(test, not(feature = "readline")))]
      Message::GetInOut => {
        // We merely relay this event to the InOutArea widget, which is
        // the only entity able to satisfy the request.
        Some(UiEvent::Directed(self.in_out, event).into())
      },
      _ => Some(UiEvent::Custom(event).into()),
    }
  }
}

impl Handleable<Event> for TermUi {
  /// Check for new input and react to it.
  fn handle(&self, _cap: &mut dyn MutCap<Event>, event: Event) -> Option<UiEvents<Event>> {
    match event {
      Event::Key(key, _) => {
        match key {
          Key::Char('q') => Some(UiEvent::Quit.into()),
          Key::Char('w') => Some(self.save()),
          _ => Some(event.into()),
        }
      },
    }
  }

  /// Handle a custom event.
  fn handle_custom(
    &self,
    cap: &mut dyn MutCap<Event>,
    event: Box<dyn Any>,
  ) -> Option<UiEvents<Event>> {
    match event.downcast::<Message>() {
      Ok(e) => self.handle_custom_event(e, cap),
      Err(e) => panic!("Received unexpected custom event: {:?}", e),
    }
  }
}


// We can't run the "end-to-end" tests in conjunction with readline
// support. Readline can be configured outside of this program's control
// and so key bindings could be arbitrary.
#[cfg(all(test, not(feature = "readline")))]
#[allow(unused_results)]
mod tests {
  use super::*;

  use gui::Ui;
  use gui::UnhandledEvent;
  use gui::UnhandledEvents;

  use crate::ser::query::Query as SerQuery;
  use crate::ser::query::TagLit as SerTagLit;
  use crate::ser::state::TaskState as SerTaskState;
  use crate::ser::state::UiState as SerUiState;
  use crate::ser::tags::Templates as SerTemplates;
  use crate::ser::tasks::Task as SerTask;
  use crate::ser::tasks::Tasks as SerTasks;
  use crate::ser::ToSerde;
  use crate::state::State;
  use crate::test::make_tasks;
  use crate::test::make_tasks_with_tags;
  use crate::test::NamedTempFile;
  use crate::ui::event::EventUpdated;
  use crate::ui::event::tests::CustomEvent;


  impl From<Key> for Event {
    fn from(key: Key) -> Self {
      Event::Key(key, ())
    }
  }

  impl From<char> for Event {
    fn from(c: char) -> Self {
      Event::Key(Key::Char(c), ())
    }
  }


  /// Create the default `UiState` with four queries and 15 tasks with
  /// tags. Tag assignment follows the pattern that
  /// `make_tasks_with_tags` creates.
  fn default_tasks_and_tags() -> (SerTaskState, SerUiState) {
    let (tags, templates, tasks) = make_tasks_with_tags(15);
    let task_state = SerTaskState {
      templates: SerTemplates(templates),
      tasks: SerTasks(tasks),
    };
    let ui_state = SerUiState {
      queries: vec![
        (SerQuery {
          name: "all".to_string(),
          lits: vec![],
        }, None),
        (SerQuery {
          name: "tag complete".to_string(),
          lits: vec![vec![SerTagLit::Pos(tags[0])]],
        }, None),
        (SerQuery {
          name: "tag2 || tag3".to_string(),
          lits: vec![
            vec![
              SerTagLit::Pos(tags[2]),
              SerTagLit::Pos(tags[3]),
            ],
          ],
        }, None),
        (SerQuery {
          name: "tag1 && tag3".to_string(),
          lits: vec![
            vec![SerTagLit::Pos(tags[1])],
            vec![SerTagLit::Pos(tags[3])],
          ],
        }, None),
      ],
      selected: None,
      colors: Default::default(),
    };

    (task_state, ui_state)
  }

  /// A builder object used for instantiating a UI with a certain
  /// composition of tasks.
  struct TestUiBuilder {
    task_state: SerTaskState,
    ui_state: SerUiState,
  }

  impl TestUiBuilder {
    /// Create a builder that will create a UI without any tasks.
    fn new() -> TestUiBuilder {
      Self {
        task_state: Default::default(),
        ui_state: Default::default(),
      }
    }

    /// Create a builder that will create a UI with the given tasks.
    fn with_ser_tasks<T>(tasks: T) -> TestUiBuilder
    where
      T: AsRef<[SerTask]> + Into<Vec<SerTask>>,
    {
      tasks.as_ref().iter().for_each(|x| assert!(x.tags.is_empty()));

      Self {
        task_state: SerTaskState {
          templates: Default::default(),
          tasks: SerTasks(tasks.into()),
        },
        ui_state: Default::default(),
      }
    }

    /// Create a builder that will instantiate a UI with state as
    /// created by `default_tasks_and_tags`.
    fn with_default_tasks_and_tags() -> TestUiBuilder {
      let (task_state, ui_state) = default_tasks_and_tags();
      TestUiBuilder {
        task_state,
        ui_state,
      }
    }

    /// Build the actual UI object that we can test with.
    fn build(self) -> TestUi {
      let task_file = NamedTempFile::new();
      let ui_file = NamedTempFile::new();
      let state = State::with_serde(
        self.task_state,
        task_file.path(),
        self.ui_state,
        ui_file.path(),
      );
      let State(task_state, ui_state) = state.unwrap();
      let path = ui_state.path.clone();

      let (ui, _) = Ui::new(
        || Box::new(TermUiData::new(task_state, path)),
        |id, cap| Box::new(TermUi::new(id, cap, ui_state)),
      );

      TestUi {
        task_file,
        ui_file,
        ui,
      }
    }
  }

  /// An UI object used for testing. It is just a handy wrapper around a
  /// `Ui`.
  #[allow(unused)]
  struct TestUi {
    task_file: NamedTempFile,
    ui_file: NamedTempFile,
    ui: Ui<Event>,
  }

  impl TestUi {
    /// Handle a single event and directly return the result.
    fn evaluate<E>(&mut self, event: E) -> Option<UnhandledEvents<Event>>
    where
      E: Into<UiEvent<Event>>,
    {
      self.ui.handle(event)
    }

    /// Send the given list of events to the UI.
    fn handle<E, I>(&mut self, events: I) -> &mut Self
    where
      E: Into<UiEvent<Event>>,
      I: IntoIterator<Item=E>,
    {
      for event in events.into_iter() {
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
      let event = UiEvent::Custom(Box::new(Message::GetInOut));
      let resp = self
        .ui
        .handle(event)
        .unwrap()
        .unwrap_custom::<Message>();

      if let Message::GotInOut(in_out) = resp {
        in_out
      } else {
        panic!("Unexpected response: {:?}", resp)
      }
    }

    /// Retrieve the names of all tabs.
    fn queries(&mut self) -> Vec<String> {
      let event = UiEvent::Custom(Box::new(Message::CollectState(false)));
      let resp = self
        .ui
        .handle(event)
        .unwrap()
        .unwrap_custom::<Message>();

      if let Message::CollectedState(tab_state) = resp {
        tab_state.queries.into_iter().map(|(query, _)| query.name().to_string()).collect()
      } else {
        panic!("Unexpected response: {:?}", resp)
      }
    }

    /// Retrieve the current set of tasks from the UI.
    fn tasks(&mut self) -> Vec<Task> {
      let event = UiEvent::Custom(Box::new(Message::GetTasks));
      let resp = self
        .ui
        .handle(event)
        .unwrap()
        .unwrap_custom::<Message>();

      if let Message::GotTasks(tasks) = resp {
        tasks
      } else {
        panic!("Unexpected response: {:?}", resp)
      }
    }

    /// Retrieve the current set of tasks in the form of `SerTask` objects from the UI.
    fn ser_tasks(&mut self) -> Vec<SerTask> {
      self.tasks().iter().map(|x| x.to_serde()).collect()
    }

    /// Load the UI's state from a file. Note that unless the state has
    /// been saved, the result will probably just be the default state.
    fn load_state(&self) -> Result<State> {
      State::new(self.task_file.path(), self.ui_file.path())
    }
  }

  #[test]
  fn exit_on_quit() {
    let events = vec![
      Event::from('q'),
      Event::from('a'),
      Event::from('f'),
      Event::from('\n'),
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
      Event::from('d'),
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
      Event::from('d'),
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
      Event::from('j'),
      Event::from('j'),
      Event::from('j'),
      Event::from('j'),
      Event::from('j'),
      Event::from('d'),
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
      Event::from('j'),
      Event::from('j'),
      Event::from('k'),
      Event::from('d'),
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
      Event::from('j'),
      Event::from('j'),
      Event::from('d'),
      Event::from('k'),
      Event::from('d'),
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
      Event::from('G'),
      Event::from('k'),
      Event::from('d'),
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
      Event::from('k'),
      Event::from('k'),
      Event::from('k'),
      Event::from('k'),
      Event::from('k'),
      Event::from('k'),
      Event::from('j'),
      Event::from('d'),
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
      Event::from('G'),
      Event::from('d'),
      Event::from('e'),
      Event::from(Key::Backspace),
      Event::from('o'),
      Event::from('\n'),
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
      Event::from('a'),
      Event::from('f'),
      Event::from('o'),
      Event::from('o'),
      Event::from('b'),
      Event::from('a'),
      Event::from('r'),
      Event::from('\n'),
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
      Event::from('a'),
      Event::from('f'),
      Event::from('o'),
      Event::from('o'),
      Event::from('\n'),
      Event::from('a'),
      Event::from('b'),
      Event::from('a'),
      Event::from('r'),
      Event::from('\n'),
      Event::from('d'),
      Event::from('d'),
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
      Event::from('a'),
      Event::from('f'),
      Event::from('o'),
      Event::from('o'),
      Event::from('b'),
      Event::from('a'),
      Event::from('z'),
      Event::from(Key::Esc),
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
      Event::from('a'),
      Event::from('f'),
      Event::from('o'),
      Event::from('o'),
      Event::from(Key::Backspace),
      Event::from(Key::Backspace),
      Event::from(Key::Backspace),
      Event::from('b'),
      Event::from('a'),
      Event::from('z'),
      Event::from('\n'),
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
      Event::from('a'),
      Event::from(Key::Delete),
      Event::from(Key::Right),
      Event::from('s'),
      Event::from('t'),
      Event::from(Key::Home),
      Event::from(Key::Home),
      Event::from('t'),
      Event::from('e'),
      Event::from(Key::End),
      Event::from(Key::End),
      Event::from(Key::Delete),
      Event::from('4'),
      Event::from('2'),
      Event::from('\n'),
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
      Event::from('a'),
      Event::from('\n'),
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
      Event::from('l'),
      Event::from('a'),
      Event::from('t'),
      Event::from('e'),
      Event::from('s'),
      Event::from('t'),
      Event::from('\n'),
      Event::from('l'),
      Event::from('l'),
      Event::from('g'),
      Event::from('a'),
      Event::from('h'),
      Event::from('i'),
      Event::from('\n'),
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
      Event::from('l'),
      Event::from('l'),
      Event::from('l'),
      Event::from('a'),
      Event::from('f'),
      Event::from('o'),
      Event::from('o'),
      Event::from('\n'),
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
      Event::from('j'),
      Event::from(' '),
      Event::from('j'),
      Event::from(' '),
      Event::from(' '),
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
      Event::from('j'),
      Event::from('e'),
      Event::from(Key::Backspace),
      Event::from('a'),
      Event::from('m'),
      Event::from('e'),
      Event::from('n'),
      Event::from('d'),
      Event::from('\n'),
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
      Event::from('j'),
      Event::from('e'),
      Event::from(Key::Esc),
      Event::from('a'),
      Event::from('f'),
      Event::from('o'),
      Event::from('o'),
      Event::from('\n'),
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
      Event::from('j'),
      Event::from('e'),
      Event::from(Key::Backspace),
      Event::from('\n'),
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
      Event::from('e'),
      Event::from('ä'),
      Event::from('ö'),
      Event::from('\n'),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(1);
    expected[0].summary = "1äö".to_string();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn edit_task_with_cursor_movement() {
    let tasks = make_tasks(3);
    let events = vec![
      Event::from('j'),
      Event::from('j'),
      Event::from('e'),
      Event::from(Key::Left),
      Event::from(Key::Left),
      Event::from(Key::Left),
      Event::from(Key::Delete),
      Event::from('t'),
      Event::from('a'),
      Event::from(Key::Left),
      Event::from(Key::Delete),
      Event::from(Key::Right),
      Event::from('e'),
      Event::from('s'),
      Event::from('t'),
      Event::from(Key::Right),
      Event::from(Key::Right),
      Event::from('\n'),
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
      Event::from('3'),
      Event::from('e'),
      Event::from('a'),
      Event::from('\n'),
      Event::from('e'),
      Event::from('b'),
      Event::from('\n'),
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
    let expected = expected.into_iter().map(|x| x.summary).collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn remove_before_multi_byte_characters() {
    let tasks = make_tasks(1);
    let events = vec![
      Event::from('e'),
      Event::from('b'),
      Event::from('ä'),
      Event::from('b'),
      Event::from(Key::Left),
      Event::from(Key::Left),
      Event::from(Key::Backspace),
      Event::from('\n'),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(1);
    expected[0].summary = "1äb".to_string();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn remove_multi_byte_characters() {
    let tasks = make_tasks(1);
    let events = vec![
      Event::from('e'),
      Event::from('b'),
      Event::from('ä'),
      Event::from('b'),
      Event::from(Key::Left),
      Event::from(Key::Left),
      Event::from(Key::Delete),
      Event::from('\n'),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .ser_tasks();

    let mut expected = make_tasks(1);
    expected[0].summary = "1bb".to_string();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn move_no_or_one_task_up_or_down() {
    fn test_tasks(count: usize, key: char) {
      let tasks = make_tasks(count);
      let events = vec![
        Event::from(key),
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
      Event::from('J'),
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
      Event::from('G'),
      Event::from('K'),
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
      Event::from('j'),
      Event::from('J'),
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
      Event::from('j'),
      Event::from('K'),
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
      Event::from('l'),
      Event::from('l'),
      Event::from('l'),
      Event::from('G'),
      Event::from('h'),
      Event::from('h'),
      Event::from('h'),
      Event::from('G'),
      Event::from('d'),
      Event::from('l'),
      Event::from('l'),
      Event::from('l'),
      // The edit should be a no-op.
      Event::from('e'),
      Event::from('\n'),
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
      Event::from('4'),
      Event::from('e'),
      Event::from('a'),
      Event::from('\n'),
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
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn select_previous_tab() {
    let events = vec![
      Event::from('2'),
      Event::from('e'),
      Event::from('a'),
      Event::from('\n'),
      Event::from('4'),
      Event::from('e'),
      Event::from('a'),
      Event::from('\n'),
      Event::from('`'),
      Event::from('j'),
      Event::from('e'),
      Event::from('a'),
      Event::from('\n'),
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
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn select_last_tab_plus_one() {
    let events = vec![
      Event::from('0'),
      Event::from('l'),
      Event::from('d'),
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
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn move_tab_left() {
    let events = vec![
      Event::from('2'),
      Event::from('H'),
    ];

    let mut ui = TestUiBuilder::with_default_tasks_and_tags().build();
    let queries = ui
      .handle(events)
      .queries();

    let expected = vec![
      "tag complete",
      "all",
      "tag2 || tag3",
      "tag1 && tag3",
    ];
    assert_eq!(queries, expected);

    // Try moving the tab once more but since we are already all the way
    // to the left nothing should change.
    let events = vec![
      Event::from('H'),
    ];
    let queries = ui
      .handle(events)
      .queries();
    assert_eq!(queries, expected);
  }

  #[test]
  fn move_tab_right() {
    let events = vec![
      Event::from('3'),
      Event::from('L'),
    ];

    let mut ui = TestUiBuilder::with_default_tasks_and_tags().build();
    let queries = ui
      .handle(events)
      .queries();

    let expected = vec![
      "all",
      "tag complete",
      "tag1 && tag3",
      "tag2 || tag3",
    ];
    assert_eq!(queries, expected);

    let events = vec![
      Event::from('L'),
    ];
    let queries = ui
      .handle(events)
      .queries();
    assert_eq!(queries, expected);
  }

  #[test]
  fn in_out_state_after_write() {
    let tasks = make_tasks(2);
    let events = vec![
      Event::from('w'),
    ];

    let state = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .in_out();

    assert_eq!(state, InOut::Saved);
  }

  #[test]
  fn in_out_state_after_write_and_key_press() {
    fn with_key(key: impl Into<Event>) -> InOut {
      let tasks = make_tasks(2);
      let events = vec![
        Event::from('w'),
        key.into().into(),
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
        assert_eq!(with_key(c), InOut::Clear, "char: {} ({})", c, c as u8);
      }
    }

    assert_eq!(with_key(Key::Esc), InOut::Clear);
    assert_eq!(with_key('\n'), InOut::Clear);
    assert_eq!(with_key(Key::PageDown), InOut::Clear);
  }

  #[test]
  fn in_out_state_on_edit() {
    fn with_key(key: impl Into<Event>) -> InOut {
      let tasks = make_tasks(4);
      let events = vec![
        Event::from('a'),
        key.into().into(),
      ];

      TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .in_out()
    }

    for c in 0u8..127u8 {
      if c == b'\n' {
        continue
      }

      let state = with_key(c);
      match state {
        InOut::Input(_, _) => (),
        _ => assert!(false, "Unexpected state {:?} for char {}", state, c),
      }
    }

    assert_eq!(with_key(Key::Esc), InOut::Clear);
    assert_eq!(with_key('\n'), InOut::Clear);
  }

  #[test]
  fn search_abort() {
    fn test(c: char) {
      let tasks = make_tasks(4);
      let events = vec![
        Event::from(c),
        Event::from('f'),
        Event::from(Key::Esc),
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
        Event::from(c),
        Event::from('f'),
        Event::from('o'),
        Event::from('o'),
        Event::from('\n'),
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
        Event::from(c),
        Event::from('f'),
        Event::from('o'),
        Event::from('o'),
        Event::from('\n'),
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
        Event::from(c),
        Event::from(c),
        Event::from(c),
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
        Event::from('/'),
        Event::from('z'),
        Event::from('\n'),
        Event::from(c),
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
      Event::from('/'),
      Event::from('2'),
      Event::from('\n'),
      Event::from('d'),
      Event::from('n'),
      Event::from('d'),
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
      Event::from('?'),
      Event::from('2'),
      Event::from('\n'),
      Event::from('d'),
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
        Event::from(c),
        Event::from('c'),
        Event::from('o'),
        Event::from('N'),
        Event::from('d'),
        Event::from('\n'),
        Event::from('d'),
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
      Event::from('j'),
      Event::from('j'),
      Event::from('j'),
      Event::from('j'),
      Event::from('j'),
      Event::from('/'),
      Event::from('1'),
      Event::from('\n'),
      Event::from('d'),
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
      Event::from('G'),
      Event::from('/'),
      Event::from('2'),
      Event::from('\n'),
      Event::from('d'),
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
        Event::from('l'),
        // Search for a task with '4' in it.
        Event::from(c1),
        Event::from('4'),
        Event::from('\n'),
        // Delete it. That should be task 14.
        Event::from('d'),
        // Move to next task.
        Event::from(c2),
        // Delete it. That should be task 4.
        Event::from('d'),
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
      let expected = expected.into_iter().map(|x| x.summary).collect::<Vec<_>>();

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
      Event::from('/'),
      Event::from('2'),
      // After this event we should have selected task '2'.
      Event::from('\n'),
      // After this event task '12'.
      Event::from('n'),
      // Rename.
      Event::from('e'),
      Event::from(Key::Backspace),
      Event::from(Key::Backspace),
      Event::from('a'),
      Event::from('a'),
      Event::from('\n'),
      // '2'
      Event::from('n'),
      // '2'
      Event::from('n'),
      Event::from('d'),
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
    let expected = expected.into_iter().map(|x| x.summary).collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  fn search_term_entry_aborted() {
    let tasks = make_tasks(5);
    let events = vec![
      Event::from('j'),
      Event::from('/'),
      Event::from('3'),
      // Abort the search term entry.
      Event::from(Key::Esc),
      // Perform a search. That shouldn't really change anything.
      Event::from('n'),
    ];

    let mut ui = TestUiBuilder::with_ser_tasks(tasks).build();
    ui.handle(events);

    assert_eq!(ui.in_out(), InOut::Error("Nothing to search for".to_string()));

    ui.handle(vec![Event::from('d')]);
    let tasks = ui.ser_tasks();
    let mut expected = make_tasks(5);
    expected.remove(1);

    assert_eq!(tasks, expected);
  }

  #[test]
  fn valid_update_events() {
    for c in 0u8..127u8 {
      let mut ui = TestUiBuilder::new().build();
      let updated = ui
        .evaluate(Event::from(c))
        .map_or(false, |x| x.is_updated());

      let c = c as char;
      let expected = c == '/' || c == '?' || c == 'a' || c == 'n' || c == 'N' || c == 'w';
      assert_eq!(updated, expected, "char: {} ({})", c, c as u8);
    }
  }

  #[test]
  fn search_no_update_without_change() {
    let mut ui = TestUiBuilder::new().build();
    let updated = ui
      .evaluate(Event::from('n'))
      .map_or(false, |x| x.is_updated());

    assert!(updated);

    let updated = ui
      .evaluate(Event::from('n'))
      .map_or(false, |x| x.is_updated());

    assert!(!updated);
  }

  #[test]
  fn save_ui_state_single_tab() {
    let tasks = make_tasks(1);
    let events = vec![
      Event::from('w'),
    ];
    let state = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .load_state()
      .unwrap()
      .1
      .to_serde();

    let expected = SerUiState {
      queries: vec![
        (SerQuery {
          name: "all".to_string(),
          lits: vec![],
        }, Some(0)),
      ],
      selected: Some(0),
      colors: Default::default(),
    };
    assert_eq!(state, expected)
  }

  #[test]
  fn save_ui_state_single_tab_different_task() {
    let tasks = make_tasks(5);
    let events = vec![
      Event::from('j'),
      Event::from('j'),
      Event::from('w'),
    ];
    let state = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .load_state()
      .unwrap()
      .1
      .to_serde();

    let expected = SerUiState {
      queries: vec![
        (SerQuery {
          name: "all".to_string(),
          lits: vec![],
        }, Some(2)),
      ],
      selected: Some(0),
      colors: Default::default(),
    };
    assert_eq!(state, expected)
  }

  #[test]
  fn save_ui_state_multiple_tabs() {
    let events = vec![
      Event::from('w'),
    ];
    let state = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .load_state()
      .unwrap()
      .1
      .to_serde();

    let (_, expected) = default_tasks_and_tags();
    assert_eq!(state.queries.len(), expected.queries.len());
    assert_eq!(state.queries.len(), 4);
    assert_eq!(state.queries[0].0.name, expected.queries[0].0.name);
    assert_eq!(state.queries[1].0.name, expected.queries[1].0.name);
    assert_eq!(state.queries[2].0.name, expected.queries[2].0.name);
    assert_eq!(state.queries[3].0.name, expected.queries[3].0.name);
    assert_eq!(state.selected, Some(0));
  }

  #[test]
  fn save_ui_state_after_various_changes() {
    let events = vec![
      Event::from('j'),
      Event::from('l'),
      Event::from('j'),
      Event::from('j'),
      Event::from('l'),
      Event::from('j'),
      Event::from('j'),
      Event::from('j'),
      Event::from('j'),
      Event::from('w'),
    ];
    let state = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .load_state()
      .unwrap()
      .1
      .to_serde();

    let (_, expected) = default_tasks_and_tags();
    assert_eq!(state.queries.len(), expected.queries.len());
    assert_eq!(state.queries.len(), 4);
    assert_eq!(state.queries[0].0.name, expected.queries[0].0.name);
    assert_eq!(state.queries[1].0.name, expected.queries[1].0.name);
    assert_eq!(state.queries[2].0.name, expected.queries[2].0.name);
    assert_eq!(state.queries[3].0.name, expected.queries[3].0.name);
    assert_eq!(state.queries[0].1, Some(1));
    assert_eq!(state.queries[1].1, Some(2));
    assert_eq!(state.queries[2].1, Some(4));
    assert_eq!(state.queries[3].1, Some(0));
    assert_eq!(state.selected, Some(2));
  }
}
