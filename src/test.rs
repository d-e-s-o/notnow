// Copyright (C) 2018-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module providing testing related utility functionality.

use crate::formula::Formula;
use crate::ser::state::TaskState as SerTaskState;
use crate::ser::state::UiConfig as SerUiConfig;
use crate::ser::tags::Id as SerId;
use crate::ser::tags::Tag as SerTag;
use crate::ser::tags::Template as SerTemplate;
use crate::ser::tags::Templates as SerTemplates;
use crate::ser::tasks::Task as SerTask;
use crate::ser::tasks::Tasks as SerTasks;
use crate::ser::tasks::TasksMeta as SerTasksMeta;
use crate::ser::view::FormulaPair as SerFormulaPair;
use crate::ser::view::View as SerView;

/// The name of a tag describing the completion state of a task.
pub const COMPLETE_TAG: &str = "complete";


/// Create `count` task objects.
pub fn make_tasks(count: usize) -> Vec<SerTask> {
  (0..count)
    .map(|i| SerTask::new(format!("{}", i + 1)))
    .collect()
}

/// Create a set of tasks that have associated tags.
///
/// Tags are assigned in the following fashion:
/// task1  -> []
/// task2  -> [complete]
/// task3  -> []
/// task4  -> [complete]
///
/// task5  -> [tag1]
/// task6  -> [tag1 + complete]
/// task7  -> [tag1]
/// task8  -> [tag1 + complete]
///
/// task9  -> [tag2]
/// task10 -> [tag2 + complete]
/// task11 -> [tag2 + tag1]
/// task12 -> [tag2 + tag1 + complete]
///
/// task13 -> [tag3]
/// task14 -> [tag3 + complete]
/// task15 -> [tag3 + tag2 + tag1]
/// task16 -> [tag3 + tag2 + tag1 + complete]
///
/// task17 -> [tag4]
/// task18 -> [tag4 + complete]
/// task19 -> [tag4 + tag3 + tag2 + tag1]
/// task20 -> [tag4 + tag3 + tag2 + tag1 + complete]
///
/// ...
pub fn make_tasks_with_tags(count: usize) -> (Vec<SerTag>, Vec<SerTemplate>, Vec<SerTask>) {
  let tags = (0..=count / 4)
    .map(|x| SerTag {
      id: SerId::try_from(x + 1).unwrap(),
    })
    .collect::<Vec<_>>();
  let templates = (0..=count / 4)
    .map(|x| {
      if x == 0 {
        SerTemplate {
          id: tags[x].id,
          name: COMPLETE_TAG.to_string(),
        }
      } else {
        SerTemplate {
          id: tags[x].id,
          name: format!("tag{x}"),
        }
      }
    })
    .collect::<Vec<_>>();
  let tasks = (0..count)
    .map(|x| {
      let mut task_tags = Vec::new();
      // Add 'complete' tag for uneven tasks.
      if x % 2 == 1 {
        task_tags.push(tags[0])
      }
      // Add the "newest" tag.
      if x >= 4 {
        task_tags.push(tags[x / 4])
      }
      // Add all previous tags.
      if x >= 8 && x % 4 >= 2 {
        task_tags.extend_from_slice(&tags[1..x / 4])
      }
      SerTask::new(format!("{}", x + 1)).with_tags(task_tags)
    })
    .collect();

  (tags, templates, tasks)
}


/// A helper function to create `SerTask` objects, just as
/// [`make_tasks`] does, but only return their summaries.
pub fn make_task_summaries(count: usize) -> Vec<String> {
  make_tasks(count)
    .into_iter()
    .map(|x| x.summary)
    .collect::<Vec<_>>()
}


/// Create the default `SerUiConfig` with four views and 15 tasks with
/// tags. Tag assignment follows the pattern that
/// `make_tasks_with_tags` creates.
pub fn default_tasks_and_tags() -> (SerUiConfig, SerTaskState) {
  let (tags, templates, tasks) = make_tasks_with_tags(15);
  let ui_config = SerUiConfig {
    views: vec![
      SerView {
        name: "all".to_string(),
        formula: SerFormulaPair::default(),
      },
      SerView {
        name: "tag complete".to_string(),
        formula: SerFormulaPair::from(Formula::Var(tags[0].id.get())),
      },
      SerView {
        name: "tag2 || tag3".to_string(),
        formula: SerFormulaPair::from(
          Formula::Var(tags[2].id.get()) | Formula::Var(tags[3].id.get()),
        ),
      },
      SerView {
        name: "tag1 && tag3".to_string(),
        formula: SerFormulaPair::from(
          Formula::Var(tags[1].id.get()) & Formula::Var(tags[3].id.get()),
        ),
      },
    ],
    colors: Default::default(),
    // The UI can be used to toggle completion state.
    toggle_tag: Some(tags[0]),
  };
  let task_state = SerTaskState {
    tasks_meta: SerTasksMeta {
      templates: SerTemplates(templates),
    },
    tasks: SerTasks::from(tasks),
  };

  (ui_config, task_state)
}
