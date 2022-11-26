// Copyright (C) 2017-2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::io::Result;
use std::path::PathBuf;

use async_trait::async_trait;

use gui::derive::Widget;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::Widget;

use crate::state::TaskState;
use crate::state::UiState;
#[cfg(all(test, not(feature = "readline")))]
use crate::tasks::Task;

use super::dialog::Dialog;
use super::dialog::DialogData;
use super::event::Event;
use super::event::Key;
use super::in_out::InOut;
use super::in_out::InOutArea;
use super::in_out::InOutAreaData;
use super::message::Message;
use super::message::MessageExt as _;
use super::tab_bar::TabBar;
use super::tab_bar::TabBarData;
use super::tab_bar::TabState;


/// The data associated with a `TermUi`.
pub struct TermUiData {
  ui_config: PathBuf,
  task_state: TaskState,
}

impl TermUiData {
  pub fn new(ui_config: PathBuf, task_state: TaskState) -> Self {
    Self {
      ui_config,
      task_state,
    }
  }
}

/// An implementation of a terminal based view.
#[derive(Debug, Widget)]
#[gui(Event = Event, Message = Message)]
pub struct TermUi {
  id: Id,
  in_out: Id,
  tab_bar: Id,
}


impl TermUi {
  /// Create a new view associated with the given `State` object.
  pub fn new(id: Id, cap: &mut dyn MutCap<Event, Message>, state: UiState) -> Self {
    let termui_id = id;
    let UiState {
      views, selected, ..
    } = state;

    // TODO: Ideally, widgets that need a modal dialog could just create
    //       one on-the-fly. But doing so will also require support for
    //       destroying widgets, which is something that the `gui` crate
    //       does not support yet.
    let dialog = cap.add_widget(
      id,
      Box::new(|| Box::new(DialogData::new())),
      Box::new(|id, cap| {
        let dialog = Dialog::new(id);
        cap.hide(id);
        Box::new(dialog)
      }),
    );
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
        Box::new(TabBar::new(id, cap, dialog, in_out, tasks, views, selected))
      }),
    );

    Self {
      id,
      in_out,
      tab_bar,
    }
  }

  /// Persist the state into a file.
  fn save_all(&self, ui_state: &UiState, task_state: &TaskState) -> Result<()> {
    // TODO: We risk data inconsistencies if the second save operation
    //       fails.
    ui_state.save()?;
    task_state.save()?;
    Ok(())
  }

  /// Save the current state.
  async fn save_and_report(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    ui_state: &UiState,
  ) -> Option<Message> {
    let data = self.data::<TermUiData>(cap);
    let in_out = match self.save_all(ui_state, &data.task_state) {
      Ok(_) => InOut::Saved,
      Err(err) => InOut::Error(format!("{}", err)),
    };

    let message = Message::SetInOut(in_out);
    cap.send(self.in_out, message).await
  }

  /// Emit an event that will eventually cause the state to be saved.
  async fn save(&self, cap: &mut dyn MutCap<Event, Message>) -> Option<Message> {
    let message = Message::CollectState;
    let state = cap.send(self.id, message).await.unwrap();

    let state = if let Message::CollectedState(state) = state {
      state
    } else {
      unreachable!()
    };

    let data = self.data::<TermUiData>(cap);
    let TabState {
      views, selected, ..
    } = state;
    let ui_state = UiState {
      path: data.ui_config.clone(),
      views,
      selected,
      colors: Default::default(),
    };
    self.save_and_report(cap, &ui_state).await
  }
}

#[async_trait(?Send)]
impl Handleable<Event, Message> for TermUi {
  /// Check for new input and react to it.
  async fn handle(&self, cap: &mut dyn MutCap<Event, Message>, event: Event) -> Option<Event> {
    match event {
      Event::Key(key, _) => match key {
        Key::Char('u') | Key::Char('U') => {
          let data = self.data::<TermUiData>(cap);
          let tasks = data.task_state.tasks();

          let to_select = if key == Key::Char('u') {
            tasks.borrow_mut().undo()
          } else {
            tasks.borrow_mut().redo()
          }?;
          if let Some(id) = to_select {
            // Ask the tab bar to select the task that was the target of
            // the undone/redone operation.
            // TODO: We may want to make sure that the `TabBar` tries to
            //       select a task on the currently selected tab first,
            //       or we run risk of spuriously flipping tabs here if
            //       the user has views that overlap in some form (i.e.,
            //       a task is displayed on multiple tabs).
            cap.send(self.tab_bar, Message::SelectTask(id, false)).await;
          }
          Some(Event::Updated)
        },
        Key::Char('q') => Some(Event::Quit),
        Key::Char('w') => self.save(cap).await.into_event(),
        // All key events not handled at this point will just get
        // swallowed.
        _ => None,
      },
      _ => Some(event),
    }
  }

  /// React to a message.
  async fn react(&self, message: Message, cap: &mut dyn MutCap<Event, Message>) -> Option<Message> {
    match message {
      Message::CollectState => {
        // We just forward the event to the TabBar.
        cap.send(self.tab_bar, message).await
      },
      #[cfg(all(test, not(feature = "readline")))]
      Message::GetTasks => {
        let data = self.data::<TermUiData>(cap);
        let tasks = data.task_state.tasks();
        let tasks = tasks
          .borrow()
          .iter()
          .map(|(_, task)| task.clone())
          .collect();
        Some(Message::GotTasks(tasks))
      },
      #[cfg(all(test, not(feature = "readline")))]
      Message::GetInOut => {
        // We merely relay this event to the InOutArea widget, which is
        // the only entity able to satisfy the request.
        cap.send(self.in_out, message).await
      },
      m => panic!("Received unexpected message: {:?}", m),
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

  use gui::Cap;
  use gui::Ui;

  use tempfile::NamedTempFile;
  use tempfile::TempDir;

  use tokio::test;

  use crate::ser::state::TaskState as SerTaskState;
  use crate::ser::state::UiState as SerUiState;
  use crate::ser::tasks::Task as SerTask;
  use crate::ser::tasks::Tasks as SerTasks;
  use crate::ser::tasks::TasksMeta as SerTasksMeta;
  use crate::ser::view::View as SerView;
  use crate::ser::ToSerde;
  use crate::state::State;
  use crate::test::default_tasks_and_tags;
  use crate::test::make_tasks;
  use crate::test::make_tasks_with_tags;


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


  /// A builder object used for instantiating a UI with a certain
  /// composition of tasks.
  struct TestUiBuilder {
    ui_state: SerUiState,
    task_state: SerTaskState,
  }

  impl TestUiBuilder {
    /// Create a builder that will create a UI without any tasks.
    fn new() -> TestUiBuilder {
      Self {
        ui_state: Default::default(),
        task_state: Default::default(),
      }
    }

    /// Create a builder that will create a UI with the given tasks.
    fn with_ser_tasks<T>(tasks: T) -> TestUiBuilder
    where
      T: AsRef<[SerTask]> + Into<Vec<SerTask>>,
    {
      tasks
        .as_ref()
        .iter()
        .for_each(|x| assert!(x.tags.is_empty()));

      Self {
        ui_state: Default::default(),
        task_state: SerTaskState {
          tasks_meta: SerTasksMeta::default(),
          templates: Default::default(),
          tasks: SerTasks(tasks.into()),
        },
      }
    }

    /// Create a builder that will instantiate a UI with state as
    /// created by `default_tasks_and_tags`.
    fn with_default_tasks_and_tags() -> TestUiBuilder {
      let (ui_state, task_state) = default_tasks_and_tags();
      TestUiBuilder {
        ui_state,
        task_state,
      }
    }

    /// Build the actual UI object that we can test with.
    fn build(self) -> TestUi {
      let ui_file = NamedTempFile::new().unwrap();
      let task_file = NamedTempFile::new().unwrap();
      let tasks_dir = TempDir::new().unwrap();
      let state = State::with_serde(
        self.ui_state,
        ui_file.path(),
        self.task_state,
        task_file.path(),
        tasks_dir.path(),
      );
      let State(ui_state, task_state) = state.unwrap();
      let path = ui_state.path.clone();

      let (ui, _) = Ui::new(
        || Box::new(TermUiData::new(path, task_state)),
        |id, cap| Box::new(TermUi::new(id, cap, ui_state)),
      );

      TestUi {
        ui,
        ui_file,
        task_file,
        tasks_root: tasks_dir,
      }
    }
  }

  /// An UI object used for testing. It is just a handy wrapper around a
  /// `Ui`.
  #[allow(unused)]
  struct TestUi {
    ui: Ui<Event, Message>,
    ui_file: NamedTempFile,
    task_file: NamedTempFile,
    tasks_root: TempDir,
  }

  impl TestUi {
    /// Handle a single event and directly return the result.
    async fn evaluate(&mut self, event: Event) -> Option<Event> {
      self.ui.handle(event).await
    }

    /// Send the given list of events to the UI.
    #[allow(unused_lifetimes)]
    async fn handle<E, I>(&mut self, events: I) -> &mut Self
    where
      E: Into<Event>,
      I: IntoIterator<Item = E>,
    {
      for event in events.into_iter() {
        if let Some(Event::Quit) = self.ui.handle(event).await {
          break
        }
      }
      self
    }

    /// Retrieve the current `InOutArea` state.
    async fn in_out(&mut self) -> InOut {
      let root = self.ui.root_id();
      let resp = self.ui.send(root, Message::GetInOut).await.unwrap();
      if let Message::GotInOut(in_out) = resp {
        in_out
      } else {
        panic!("Unexpected response: {:?}", resp)
      }
    }

    /// Retrieve the names of all tabs.
    async fn views(&mut self) -> Vec<String> {
      let root = self.ui.root_id();
      let resp = self.ui.send(root, Message::CollectState).await.unwrap();

      if let Message::CollectedState(tab_state) = resp {
        tab_state
          .views
          .into_iter()
          .map(|(view, _)| view.name().to_string())
          .collect()
      } else {
        panic!("Unexpected response: {:?}", resp)
      }
    }

    /// Retrieve the current set of tasks from the UI.
    async fn tasks(&mut self) -> Vec<Task> {
      let root = self.ui.root_id();
      let resp = self.ui.send(root, Message::GetTasks).await.unwrap();
      if let Message::GotTasks(tasks) = resp {
        tasks
      } else {
        panic!("Unexpected response: {:?}", resp)
      }
    }

    /// Retrieve the current set of tasks in the form of `SerTask` objects from the UI.
    async fn ser_tasks(&mut self) -> Vec<SerTask> {
      self.tasks().await.iter().map(|x| x.to_serde()).collect()
    }

    /// Load the UI's state from a file. Note that unless the state has
    /// been saved, the result will probably just be the default state.
    fn load_state(&self) -> Result<State> {
      State::new(
        self.ui_file.path(),
        self.task_file.path(),
        self.tasks_root.path(),
      )
    }
  }

  #[test]
  async fn exit_on_quit() {
    let events = vec![
      Event::from('q'),
      Event::from('a'),
      Event::from('f'),
      Event::from('\n'),
    ];

    let tasks = TestUiBuilder::new()
      .build()
      .handle(events)
      .await
      .ser_tasks()
      .await;

    assert_eq!(tasks, make_tasks(0))
  }

  #[test]
  async fn remove_no_task() {
    let events = vec![Event::from('d')];

    let tasks = TestUiBuilder::new()
      .build()
      .handle(events)
      .await
      .ser_tasks()
      .await;

    assert_eq!(tasks, make_tasks(0))
  }

  #[test]
  async fn remove_only_task() {
    let tasks = make_tasks(1);
    let events = vec![Event::from('d')];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .await
      .ser_tasks()
      .await;

    assert_eq!(tasks, make_tasks(0))
  }

  #[test]
  async fn remove_task_after_down_select() {
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
      .await
      .ser_tasks()
      .await;

    assert_eq!(tasks, make_tasks(1))
  }

  #[test]
  async fn remove_task_after_up_select() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(3);
    expected.remove(1);
    assert_eq!(tasks, expected)
  }

  #[test]
  async fn remove_last_task() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(3);
    expected.remove(0);
    expected.remove(1);
    assert_eq!(tasks, expected)
  }

  #[test]
  async fn remove_second_to_last_task() {
    let tasks = make_tasks(5);
    let events = vec![Event::from('G'), Event::from('k'), Event::from('d')];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(5);
    expected.remove(3);
    assert_eq!(tasks, expected)
  }

  #[test]
  async fn remove_second_task_after_up_select() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(3);
    expected.remove(1);
    assert_eq!(tasks, expected)
  }

  #[test]
  async fn selection_after_removal() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(4);
    expected[3].summary = "o".to_string();
    assert_eq!(tasks, expected)
  }

  #[test]
  async fn add_task() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(1);
    expected[0].summary = "foobar".to_string();

    assert_eq!(tasks, expected)
  }

  #[test]
  async fn add_and_remove_tasks() {
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
      .await
      .ser_tasks()
      .await;

    assert_eq!(tasks, make_tasks(0))
  }

  #[test]
  async fn add_task_cancel() {
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
      .await
      .ser_tasks()
      .await;

    assert_eq!(tasks, make_tasks(0))
  }

  #[test]
  async fn add_task_with_character_removal() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(2);
    expected[1].summary = "baz".to_string();

    assert_eq!(tasks, expected)
  }

  #[test]
  async fn add_task_with_cursor_movement() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(2);
    expected[1].summary = "test42".to_string();

    assert_eq!(tasks, expected)
  }

  #[test]
  async fn add_empty_task() {
    let tasks = make_tasks(1);
    let events = vec![Event::from('a'), Event::from('\n')];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .await
      .ser_tasks()
      .await;

    assert_eq!(tasks, make_tasks(1))
  }

  #[test]
  async fn add_task_from_completed_one() {
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
      Event::from('j'),
      Event::from('a'),
      Event::from('h'),
      Event::from('i'),
      Event::from('\n'),
    ];

    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .await
      .tasks()
      .await;

    // We created a task on the second tab, i.e., the one showing
    // completed tasks (which is every other). So we expect the new task
    // to show up past the second one in the list of all tests, at index
    // 2.
    assert_eq!(tasks[2].summary, "test");

    // Check that the 'complete' tag, which was present on the original
    // task, has been cleared.
    let tags = tasks[2].tags().map(|x| x.name()).collect::<Vec<_>>();
    let expected = Vec::<String>::new();
    assert_eq!(tags, expected);

    // The second task should have been created on the third tab,
    // which shows tasks with tag2 or tag3 present.
    assert_eq!(tasks[11].summary, "hi");

    let tags = tasks[11].tags().map(|x| x.name()).collect::<Vec<_>>();
    let expected = vec!["tag2"];
    assert_eq!(tags, expected);
  }

  #[test]
  async fn add_task_with_tags() {
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
      .await
      .tasks()
      .await;

    assert_eq!(tasks[15].summary, "foo");

    let tags = tasks[15].tags().map(|x| x.name()).collect::<Vec<_>>();
    let expected = vec!["tag1".to_string(), "tag2".to_string(), "tag3".to_string()];
    assert_eq!(tags, expected);
  }

  #[test]
  async fn complete_task() {
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
      .await
      .tasks()
      .await;

    assert!(!tasks[0].is_complete());
    assert!(tasks[1].is_complete());
    assert!(!tasks[2].is_complete());
  }

  #[test]
  async fn edit_task() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(3);
    expected[1].summary = "amend".to_string();

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn edit_task_cancel() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(3);
    expected.insert(2, SerTask::new("foo"));

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn edit_task_to_empty() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(3);
    expected.remove(1);

    assert_eq!(tasks, expected)
  }

  #[test]
  async fn edit_task_multi_byte_char() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(1);
    expected[0].summary = "1äö".to_string();

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn edit_task_with_cursor_movement() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(3);
    expected[2].summary = "test".to_string();

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn edit_multiple_tasks() {
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
      .await
      .tasks()
      .await
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    let (.., mut expected) = make_tasks_with_tags(15);
    expected[8].summary = "9ab".to_string();
    let expected = expected.into_iter().map(|x| x.summary).collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn remove_before_multi_byte_characters() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(1);
    expected[0].summary = "1äb".to_string();

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn remove_multi_byte_characters() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(1);
    expected[0].summary = "1bb".to_string();

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn move_no_or_one_task_up_or_down() {
    async fn test_tasks(count: usize, key: char) {
      let tasks = make_tasks(count);
      let events = vec![Event::from(key)];

      let tasks = TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .await
        .ser_tasks()
        .await;

      assert_eq!(tasks, make_tasks(count));
    }

    test_tasks(0, 'J').await;
    test_tasks(1, 'J').await;
    test_tasks(0, 'K').await;
    test_tasks(1, 'K').await;
  }

  #[test]
  async fn move_task_down() {
    let tasks = make_tasks(3);
    let events = vec![Event::from('J')];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(3);
    expected.swap(0, 1);

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn move_task_up() {
    let tasks = make_tasks(3);
    let events = vec![Event::from('G'), Event::from('K')];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(3);
    expected.swap(2, 1);

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn move_second_task_down() {
    let tasks = make_tasks(4);
    let events = vec![Event::from('j'), Event::from('J')];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(4);
    expected.swap(1, 2);

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn move_second_task_up() {
    let tasks = make_tasks(4);
    let events = vec![Event::from('j'), Event::from('K')];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(4);
    expected.swap(1, 0);

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn transparent_task_removal_down_to_empty_view() {
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
      .await
      .tasks()
      .await;

    assert_eq!(tasks.len(), 14);
  }

  #[test]
  async fn tab_selection_by_number() {
    let events = vec![
      Event::from('4'),
      Event::from('e'),
      Event::from('a'),
      Event::from('\n'),
    ];

    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .await
      .tasks()
      .await
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    let (.., mut expected) = make_tasks_with_tags(15);
    expected[14].summary = "15a".to_string();
    let expected = expected.into_iter().map(|x| x.summary).collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn select_previous_tab() {
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
      .await
      .tasks()
      .await
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    let (.., mut expected) = make_tasks_with_tags(15);
    expected[1].summary = "2a".to_string();
    expected[3].summary = "4a".to_string();
    expected[14].summary = "15a".to_string();
    let expected = expected.into_iter().map(|x| x.summary).collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn select_last_tab_plus_one() {
    let events = vec![Event::from('0'), Event::from('l'), Event::from('d')];

    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .await
      .tasks()
      .await
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    let (.., mut expected) = make_tasks_with_tags(15);
    expected.remove(14);
    let expected = expected.into_iter().map(|x| x.summary).collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn move_tab_left() {
    let events = vec![Event::from('2'), Event::from('H')];

    let mut ui = TestUiBuilder::with_default_tasks_and_tags().build();
    let views = ui.handle(events).await.views().await;

    let expected = vec!["tag complete", "all", "tag2 || tag3", "tag1 && tag3"];
    assert_eq!(views, expected);

    // Try moving the tab once more but since we are already all the way
    // to the left nothing should change.
    let events = vec![Event::from('H')];
    let views = ui.handle(events).await.views().await;
    assert_eq!(views, expected);
  }

  #[test]
  async fn move_tab_right() {
    let events = vec![Event::from('3'), Event::from('L')];

    let mut ui = TestUiBuilder::with_default_tasks_and_tags().build();
    let views = ui.handle(events).await.views().await;

    let expected = vec!["all", "tag complete", "tag1 && tag3", "tag2 || tag3"];
    assert_eq!(views, expected);

    let events = vec![Event::from('L')];
    let views = ui.handle(events).await.views().await;
    assert_eq!(views, expected);
  }

  #[test]
  async fn in_out_state_after_write() {
    let tasks = make_tasks(2);
    let events = vec![Event::from('w')];

    let state = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .await
      .in_out()
      .await;

    assert_eq!(state, InOut::Saved);
  }

  #[test]
  async fn in_out_state_after_write_and_key_press() {
    async fn with_key(key: impl Into<Event>) -> InOut {
      let tasks = make_tasks(2);
      let events = vec![Event::from('w'), key.into()];

      TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .await
        .in_out()
        .await
    }

    // We test all ASCII chars.
    for c in 0u8..127u8 {
      let c = c as char;
      if c != 'a'
        && c != 'e'
        && c != 'n'
        && c != 'N'
        && c != 't'
        && c != 'w'
        && c != '/'
        && c != '?'
        && c != '*'
      {
        assert_eq!(with_key(c).await, InOut::Clear, "char: {} ({})", c, c as u8);
      }
    }

    assert_eq!(with_key(Key::Esc).await, InOut::Clear);
    assert_eq!(with_key('\n').await, InOut::Clear);
    assert_eq!(with_key(Key::PageDown).await, InOut::Clear);
  }

  #[test]
  async fn updated_event_after_write_and_key_press() {
    let tasks = make_tasks(2);
    let updated = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(Some(Event::from('w')))
      .await
      .evaluate(Event::from('y'))
      .await
      .map_or(false, |x| x.is_updated());

    assert!(updated);
  }

  #[test]
  async fn in_out_state_on_edit() {
    async fn with_key(key: impl Into<Event>) -> InOut {
      let tasks = make_tasks(4);
      let events = vec![Event::from('a'), key.into()];

      TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .await
        .in_out()
        .await
    }

    for c in 0u8..127u8 {
      if c == b'\n' {
        continue
      }

      let state = with_key(c).await;
      match state {
        InOut::Input(_, _) => (),
        _ => panic!("Unexpected state {:?} for char {}", state, c),
      }
    }

    assert_eq!(with_key(Key::Esc).await, InOut::Clear);
    assert_eq!(with_key('\n').await, InOut::Clear);
  }

  #[test]
  async fn search_empty_tab() {
    async fn test(c: char) {
      let events = vec![Event::from(c), Event::from('f'), Event::from('\n')];

      let state = TestUiBuilder::with_ser_tasks(Vec::new())
        .build()
        .handle(events)
        .await
        .in_out()
        .await;

      let expected = InOut::Error("Text 'f' not found".to_string());
      assert_eq!(state, expected);
    }

    test('/').await;
    test('?').await;
  }

  #[test]
  async fn search_single_task() {
    async fn test(c: char) {
      let tasks = make_tasks(1);
      let events = vec![Event::from(c), Event::from('1'), Event::from('\n')];

      let state = TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .await
        .in_out()
        .await;

      let expected = InOut::Search("1".to_string());
      assert_eq!(state, expected);
    }

    test('/').await;
    test('?').await;
  }

  #[test]
  async fn search_abort() {
    async fn test(c: char) {
      let tasks = make_tasks(4);
      let events = vec![Event::from(c), Event::from('f'), Event::from(Key::Esc)];

      let state = TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .await
        .in_out()
        .await;

      assert_eq!(state, InOut::Clear);
    }

    test('/').await;
    test('?').await;
  }

  #[test]
  async fn search_tasks_not_found() {
    async fn test(c: char) {
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
        .await
        .in_out()
        .await;

      let expected = InOut::Error("Text 'foo' not found".to_string());
      assert_eq!(state, expected);
    }

    test('/').await;
    test('?').await;
  }

  #[test]
  async fn search_multiple_tabs_not_found() {
    async fn test(c: char) {
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
        .await
        .in_out()
        .await;

      let expected = InOut::Error("Text 'foo' not found".to_string());
      assert_eq!(state, expected);
    }

    test('/').await;
    test('?').await;
  }

  #[test]
  async fn search_continue_without_start() {
    async fn test(c: char) {
      let tasks = make_tasks(4);
      let events = vec![Event::from(c), Event::from(c), Event::from(c)];

      let state = TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .await
        .in_out()
        .await;

      let expected = InOut::Error("Nothing to search for".to_string());
      assert_eq!(state, expected);
    }

    test('n').await;
    test('N').await;
  }

  #[test]
  async fn search_tasks_repeatedly_not_found() {
    async fn test(c: char) {
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
        .await
        .in_out()
        .await;

      let expected = InOut::Error("Text 'z' not found".to_string());
      assert_eq!(state, expected);
    }

    test('n').await;
    test('N').await;
  }

  #[test]
  async fn search_tasks() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(12);
    expected.remove(11);
    expected.remove(1);

    assert_eq!(tasks, expected);
  }

  /// Check that we can search in reverse direction.
  #[test]
  async fn search_reverse() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(12);
    expected.remove(11);

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn search_case_insensitive() {
    async fn test(c: char) {
      let tasks = vec![SerTask::new("First"), SerTask::new("SeCOnd")];
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
        .await
        .ser_tasks()
        .await;

      let expected = vec![SerTask::new("First")];
      assert_eq!(tasks, expected);
    }

    test('/').await;
    test('?').await;
  }

  #[test]
  async fn search_starting_at_selection() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(15);
    expected.remove(9);

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn search_overlapping() {
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
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(5);
    expected.remove(1);

    assert_eq!(tasks, expected);
  }

  /// Check that we can search tasks across multiple tabs.
  #[test]
  async fn search_tasks_on_multiple_tabs() {
    async fn test(c1: char, c2: char) {
      let events = vec![
        // Switch to 'complete' tab.
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
        .await
        .tasks()
        .await
        .into_iter()
        .map(|x| x.summary)
        .collect::<Vec<_>>();

      let (.., mut expected) = make_tasks_with_tags(15);
      expected.remove(13);
      expected.remove(3);
      let expected = expected.into_iter().map(|x| x.summary).collect::<Vec<_>>();

      assert_eq!(tasks, expected);
    }

    test('/', 'n').await;
    test('/', 'N').await;
    test('?', 'n').await;
    test('?', 'N').await;
  }

  /// Check that we can search across multiple tabs in reverse
  /// direction, This test is a regression test whose semantics
  /// should not be changed.
  #[test]
  async fn search_tasks_reverse_on_multiple_tabs() {
    let events = vec![
      // Search for a task with '3' in it. The result should be task 3.
      Event::from('/'),
      Event::from('3'),
      Event::from('\n'),
      // Mark it as complete so that it appears on the 'complete' tab.
      Event::from(' '),
      // Search forward. This action will select task 13 on tab 'tag2 ||
      // tag3'.
      Event::from('n'),
      Event::from('n'),
      Event::from('n'),
      // Move it to the top.
      Event::from('K'),
      Event::from('K'),
      Event::from('K'),
      Event::from('K'),
      // Now search reverse. That should select task 3 on the 'complete'
      // tab.
      Event::from('N'),
      // Delete it.
      Event::from('d'),
    ];

    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .await
      .tasks()
      .await
      .into_iter()
      .map(|x| x.summary)
      .collect::<Vec<_>>();

    let (.., mut expected) = make_tasks_with_tags(15);
    let task = expected.remove(12);
    expected.insert(8, task);
    expected.remove(2);
    let expected = expected.into_iter().map(|x| x.summary).collect::<Vec<_>>();

    assert_eq!(tasks, expected);
  }

  #[test]
  async fn search_wrap_around() {
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
      .await
      .tasks()
      .await
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
  async fn search_term_entry_aborted() {
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
    ui.handle(events).await;

    assert_eq!(
      ui.in_out().await,
      InOut::Error("Nothing to search for".to_string())
    );

    ui.handle(vec![Event::from('d')]).await;
    let tasks = ui.ser_tasks().await;
    let mut expected = make_tasks(5);
    expected.remove(1);

    assert_eq!(tasks, expected);
  }

  /// Check that we can easily search for the currently selected task on
  /// other tabs.
  #[test]
  async fn search_current_task() {
    async fn test(count: usize) {
      let down = (1..count).map(|_| Event::from('j'));
      let events = vec![
        // Search for it on other tabs. That should find it on the very
        // next tab (the one showing all tasks with the complete tag).
        Event::from('*'),
        // Now delete the follow on task. This should be task10 (because
        // task9 does not have the complete tag).
        Event::from('j'),
        Event::from('d'),
      ];
      let events = down.chain(events);

      let tasks = TestUiBuilder::with_default_tasks_and_tags()
        .build()
        .handle(events)
        .await
        .tasks()
        .await
        .into_iter()
        .map(|x| x.summary)
        .collect::<Vec<_>>();

      let (.., mut expected) = make_tasks_with_tags(15);
      expected.remove(count);
      let expected = expected.into_iter().map(|x| x.summary).collect::<Vec<_>>();

      assert_eq!(tasks, expected);
    }

    test(9).await;
    // Note that this test actually tests exact matching, because
    // without exact matching we would select and delete a different
    // task.
    test(1).await;
  }

  /// Check that a "current task search" is case-sensitive and exact
  /// when started but also when resumed.
  #[test]
  async fn search_current_is_exact_and_case_sensitive() {
    async fn test(tasks: Vec<SerTask>) {
      let mut expected = tasks.iter().map(|x| x.summary.clone()).collect::<Vec<_>>();
      expected[0] += "d";
      expected.remove(1);

      let events = vec![
        Event::from('*'),
        Event::from('e'),
        Event::from('d'),
        Event::from('\n'),
        Event::from('j'),
        // Resume search. We should not find anything.
        Event::from('n'),
        // Delete the current task, which is "test".
        Event::from('d'),
      ];

      let tasks = TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .await
        .tasks()
        .await
        .into_iter()
        .map(|x| x.summary)
        .collect::<Vec<_>>();

      assert_eq!(tasks, expected);
    }

    let tasks = vec![
      SerTask::new("LOWER"),
      SerTask::new("test"),
      SerTask::new("LOWER still"),
      SerTask::new("lower"),
      SerTask::new("lowered"),
    ];
    test(tasks).await;

    let tasks = vec![
      SerTask::new("lower"),
      SerTask::new("test"),
      SerTask::new("LOWER still"),
      SerTask::new("LOWER"),
      SerTask::new("lowered"),
    ];
    test(tasks).await;
  }

  #[test]
  async fn valid_update_events() {
    for c in 0u8..127u8 {
      let mut ui = TestUiBuilder::new().build();
      let updated = ui
        .evaluate(Event::from(c))
        .await
        .map_or(false, |x| x.is_updated());

      let c = c as char;
      let expected = c == '/' || c == '?' || c == 'a' || c == 'n' || c == 'N' || c == 'w';
      assert_eq!(updated, expected, "char: {} ({})", c, c as u8);
    }
  }

  #[test]
  async fn search_no_update_without_change() {
    let mut ui = TestUiBuilder::new().build();
    let updated = ui
      .evaluate(Event::from('n'))
      .await
      .map_or(false, |x| x.is_updated());

    assert!(updated);

    let updated = ui
      .evaluate(Event::from('n'))
      .await
      .map_or(false, |x| x.is_updated());

    assert!(!updated);
  }

  #[test]
  async fn save_ui_state_single_tab() {
    let tasks = make_tasks(1);
    let events = vec![Event::from('w')];
    let state = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .await
      .load_state()
      .unwrap()
      .0
      .to_serde();

    let expected = SerUiState {
      views: vec![(
        SerView {
          name: "all".to_string(),
          lits: vec![],
        },
        Some(0),
      )],
      selected: Some(0),
      colors: Default::default(),
    };
    assert_eq!(state, expected)
  }

  #[test]
  async fn save_ui_state_single_tab_different_task() {
    let tasks = make_tasks(5);
    let events = vec![Event::from('j'), Event::from('j'), Event::from('w')];
    let state = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .await
      .load_state()
      .unwrap()
      .0
      .to_serde();

    let expected = SerUiState {
      views: vec![(
        SerView {
          name: "all".to_string(),
          lits: vec![],
        },
        Some(2),
      )],
      selected: Some(0),
      colors: Default::default(),
    };
    assert_eq!(state, expected)
  }

  #[test]
  async fn save_ui_state_multiple_tabs() {
    let events = vec![Event::from('w')];
    let state = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .await
      .load_state()
      .unwrap()
      .0
      .to_serde();

    let (expected, _) = default_tasks_and_tags();
    assert_eq!(state.views.len(), expected.views.len());
    assert_eq!(state.views.len(), 4);
    assert_eq!(state.views[0].0.name, expected.views[0].0.name);
    assert_eq!(state.views[1].0.name, expected.views[1].0.name);
    assert_eq!(state.views[2].0.name, expected.views[2].0.name);
    assert_eq!(state.views[3].0.name, expected.views[3].0.name);
    assert_eq!(state.selected, Some(0));
  }

  #[test]
  async fn save_ui_state_after_various_changes() {
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
      .await
      .load_state()
      .unwrap()
      .0
      .to_serde();

    let (expected, _) = default_tasks_and_tags();
    assert_eq!(state.views.len(), expected.views.len());
    assert_eq!(state.views.len(), 4);
    assert_eq!(state.views[0].0.name, expected.views[0].0.name);
    assert_eq!(state.views[1].0.name, expected.views[1].0.name);
    assert_eq!(state.views[2].0.name, expected.views[2].0.name);
    assert_eq!(state.views[3].0.name, expected.views[3].0.name);
    assert_eq!(state.views[0].1, Some(1));
    assert_eq!(state.views[1].1, Some(2));
    assert_eq!(state.views[2].1, Some(4));
    assert_eq!(state.views[3].1, Some(0));
    assert_eq!(state.selected, Some(2));
  }

  /// Check that we can edit the tags of a task with no tags set
  /// currently.
  #[test]
  async fn edit_task_with_no_tags() {
    let events = vec![
      Event::from('t'),
      // Set the first tag, which is "complete".
      Event::from(' '),
      Event::from('\n'),
    ];
    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .await
      .tasks()
      .await;

    let tags = tasks[0].tags().map(|x| x.name()).collect::<Vec<_>>();
    let expected = vec!["complete"];
    assert_eq!(tags, expected);
  }

  /// Check that we can edit the tags of a task with some tags set
  /// already.
  #[test]
  async fn edit_task_with_existing_tags() {
    let events = vec![
      // Move to the third tab (tag2 || tag3).
      Event::from('l'),
      Event::from('l'),
      // Move to task11.
      Event::from('j'),
      Event::from('j'),
      Event::from('t'),
      // Move to tag2.
      Event::from('j'),
      // Toggle it.
      Event::from(' '),
      // Move to tag1.
      Event::from('k'),
      // Toggle it.
      Event::from(' '),
      Event::from('\n'),
    ];
    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .await
      .tasks()
      .await;

    let tags = tasks[10].tags().map(|x| x.name()).collect::<Vec<_>>();
    assert_eq!(tags, Vec::<&str>::new());
  }

  /// Test setting all available tags for a task.
  #[test]
  async fn set_all_tags() {
    let events = vec![
      // Move to task3.
      Event::from('j'),
      Event::from('j'),
      Event::from('t'),
      Event::from(' '),
      Event::from('j'),
      Event::from(' '),
      Event::from('j'),
      Event::from(' '),
      Event::from('j'),
      Event::from(' '),
      Event::from('\n'),
    ];
    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .await
      .tasks()
      .await;

    let tags = tasks[2].tags().map(|x| x.name()).collect::<Vec<_>>();
    let expected = vec!["complete", "tag1", "tag2", "tag3"];
    assert_eq!(tags, expected);
  }

  /// Check that we can select the first and last tags quickly.
  #[test]
  async fn first_last_tag() {
    let events = vec![
      Event::from('t'),
      Event::from('j'),
      // Toggle the last tag.
      Event::from('G'),
      Event::from(' '),
      // Toggle the first tag.
      Event::from('g'),
      Event::from(' '),
      Event::from('\n'),
    ];
    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .await
      .tasks()
      .await;

    let tags = tasks[0].tags().map(|x| x.name()).collect::<Vec<_>>();
    let expected = vec!["complete", "tag3"];
    assert_eq!(tags, expected);
  }

  /// Check that we can quickly jump to a certain tag on the dialog.
  #[test]
  async fn jump_to_tag() {
    let events = vec![
      Event::from('t'),
      Event::from('f'),
      Event::from('t'),
      Event::from(' '),
      Event::from('F'),
      Event::from('c'),
      Event::from(' '),
      Event::from('\n'),
    ];
    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .await
      .tasks()
      .await;

    let tags = tasks[0].tags().map(|x| x.name()).collect::<Vec<_>>();
    let expected = vec!["complete", "tag1"];
    assert_eq!(tags, expected);
  }

  /// Check that we can properly abort a "jump to" action.
  #[test]
  async fn jump_to_tag_abort() {
    let events = vec![
      Event::from('t'),
      Event::from('f'),
      Event::from(Key::Esc),
      Event::from('t'),
      Event::from(' '),
      Event::from('\n'),
    ];
    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .await
      .tasks()
      .await;

    let tags = tasks[0].tags().map(|x| x.name()).collect::<Vec<_>>();
    let expected = vec!["complete"];
    assert_eq!(tags, expected);
  }

  /// Check that a task is re-selected after its tags were changed.
  #[test]
  async fn tag_change_reselects_task() {
    let events = vec![
      // Move to the "complete" tab.
      Event::from('l'),
      // Move to task10.
      Event::from('j'),
      Event::from('j'),
      Event::from('j'),
      Event::from('j'),
      Event::from('t'),
      // Deselect its complete tag.
      Event::from(' '),
      Event::from('\n'),
      // Edit the task and append 'ab' to it.
      Event::from('e'),
      Event::from('a'),
      Event::from('b'),
      Event::from('\n'),
    ];
    let tasks = TestUiBuilder::with_default_tasks_and_tags()
      .build()
      .handle(events)
      .await
      .tasks()
      .await;

    assert_eq!(tasks[9].summary, "10ab");
  }

  /// Check that we can undo a task removal.
  #[test]
  async fn undo_task_removal() {
    let tasks = make_tasks(4);
    let events = vec![
      Event::from('j'),
      Event::from('d'),
      Event::from('j'),
      Event::from('d'),
      Event::from('u'),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(4);
    expected.remove(1);
    assert_eq!(tasks, expected);
  }

  /// Check that we can undo and then redo a task removal.
  #[test]
  async fn redo_task_removal() {
    let tasks = make_tasks(4);
    let events = vec![
      Event::from('j'),
      Event::from('d'),
      Event::from('u'),
      Event::from('U'),
    ];

    let tasks = TestUiBuilder::with_ser_tasks(tasks)
      .build()
      .handle(events)
      .await
      .ser_tasks()
      .await;

    let mut expected = make_tasks(4);
    expected.remove(1);
    assert_eq!(tasks, expected);
  }

  /// Check that the correct task is selected when a task removal is
  /// undone.
  #[test]
  async fn select_on_task_op_undo() {
    async fn test(events: impl IntoIterator<Item = Event>, redo: bool, add: bool) {
      let tasks = make_tasks(4);
      let before = vec![Event::from('j')];
      let after = if !redo {
        vec![
          Event::from('G'),
          // Undo the task operation. The second task should be selected at
          // this point.
          Event::from('u'),
          // Delete the currently selected task.
          Event::from('d'),
        ]
      } else {
        vec![
          Event::from('u'),
          Event::from('G'),
          // Redo the task addition. The second task should be selected at
          // this point.
          Event::from('U'),
          // Delete the currently selected task.
          Event::from('d'),
        ]
      };
      let events = before.into_iter().chain(events).chain(after);

      let tasks = TestUiBuilder::with_ser_tasks(tasks)
        .build()
        .handle(events)
        .await
        .ser_tasks()
        .await;

      let mut expected = make_tasks(4);
      if !add {
        expected.remove(1);
      }
      assert_eq!(tasks, expected);
    }

    // Test undo of task deletion.
    test(vec![Event::from('d')], false, false).await;
    // Test undo and redo of task update.
    for redo in &[false, true] {
      test(
        vec![Event::from('e'), Event::from('1'), Event::from('\n')],
        *redo,
        false,
      )
      .await;
      // Test undo and redo of task move.
      test(vec![Event::from('J')], *redo, false).await;
    }
    // Test redo of task addition.
    test(
      vec![
        Event::from('a'),
        Event::from('f'),
        Event::from('o'),
        Event::from('o'),
        Event::from('\n'),
      ],
      true,
      true,
    )
    .await;
  }
}
