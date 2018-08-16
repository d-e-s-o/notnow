// tab_bar.rs

// *************************************************************************
// * Copyright (C) 2018 Daniel Mueller (deso@posteo.net)                   *
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

use std::cmp::max;
use std::cmp::min;

use gui::Cap;
use gui::Event;
use gui::Handleable;
use gui::Id;
use gui::Key;
use gui::MetaEvent;
use gui::UiEvent;

use event::EventUpdated;
use state::State;
use task_list_box::TaskListBox;
use termui::TermUiEvent;


/// Sanitize a selection index.
fn sanitize_selection(selection: isize, count: usize) -> usize {
  max(0, min(count as isize - 1, selection)) as usize
}


/// A widget representing a tabbed container for other widgets.
#[derive(Debug, GuiWidget)]
pub struct TabBar {
  id: Id,
  tabs: Vec<(String, Id)>,
  selection: usize,
}

impl TabBar {
  /// Create a new `TabBar` widget.
  pub fn new(id: Id, cap: &mut Cap, state: &State) -> Self {
    let selection = 0;
    // TODO: We really should not be cloning the queries to use here.
    let tabs = state
      .queries()
      .cloned()
      .enumerate()
      .map(|(i, query)| {
        let name = query.name().to_string();
        let mut query = Some(query);
        let task_list = cap.add_widget(id, &mut |id, _cap| {
          Box::new(TaskListBox::new(id, query.take().unwrap()))
        });

        if i == selection {
          cap.focus(task_list);
        } else {
          cap.hide(task_list);
        }
        (name, task_list)
      }).collect();

    TabBar {
      id: id,
      tabs: tabs,
      selection: selection,
    }
  }

  /// Handle a custom event.
  fn handle_custom_event(&mut self, event: Box<TermUiEvent>, cap: &mut Cap) -> Option<MetaEvent> {
    match *event {
      TermUiEvent::AddTaskResp(task_id) => {
        // A task got added. Now we need to select it. We do not know
        // which of the tabs may display this task, but we start
        // checking with the currently selected one.
        let tab = self.selected_tab();
        let event = Box::new(TermUiEvent::SelectTask(task_id, None));
        let event = UiEvent::Custom(tab, event);
        Some(MetaEvent::UiEvent(event))
      },
      TermUiEvent::SelectTask(task_id, widget_id) => {
        let next_idx = if let Some(widget_id) = widget_id {
          // The widget we tried was not able to select the given task.
          // Forward it to the next tab in line.
          self.tabs.iter().position(|x| x.1 == widget_id).unwrap() + 1
        } else {
          // If `widget_id` was None we started off with the selected
          // tab and now want to continue with the first one in line.
          0
        };

        let next_tab = self.tabs.get(next_idx).map(|x| x.1);
        if let Some(next_tab) = next_tab {
          let event = Box::new(TermUiEvent::SelectTask(task_id, Some(next_tab)));
          let event = UiEvent::Custom(next_tab, event);
          Some(MetaEvent::UiEvent(event))
        } else {
          None
        }
      },
      TermUiEvent::SelectedTask(widget_id) => {
        let select = self.tabs.iter().position(|x| x.1 == widget_id).unwrap();
        let update = self.set_select(select as isize, cap);
        (None as Option<Event>).maybe_update(update)
      },
      _ => Some(Event::Custom(event).into()),
    }
  }

  /// Retrieve an iterator over the names of all the tabs.
  pub fn iter(&self) -> impl Iterator<Item=&String> {
    self.tabs.iter().map(|(x, _)| x)
  }

  /// Retrieve the index of the currently selected tab.
  pub fn selection(&self) -> usize {
    self.selection
  }

  /// Retrieve the `Id` of the selected tab.
  fn selected_tab(&self) -> Id {
    self.tabs[self.selection].1
  }

  /// Change the currently selected tab.
  fn set_select(&mut self, new_selection: isize, cap: &mut Cap) -> bool {
    let count = self.iter().count();
    let old_selection = self.selection;
    let new_selection = sanitize_selection(new_selection, count);

    if new_selection != old_selection {
      cap.hide(self.selected_tab());
      self.selection = new_selection;
      cap.focus(self.selected_tab());
      true
    } else {
      false
    }
  }

  /// Change the currently selected tab.
  fn select(&mut self, change: isize, cap: &mut Cap) -> bool {
    let new_selection = self.selection as isize + change;
    self.set_select(new_selection, cap)
  }
}

impl Handleable for TabBar {
  /// Check for new input and react to it.
  fn handle(&mut self, event: Event, cap: &mut Cap) -> Option<MetaEvent> {
    match event {
      Event::KeyDown(key) |
      Event::KeyUp(key) => {
        match key {
          Key::Char('h') => (None as Option<Event>).maybe_update(self.select(-1, cap)),
          Key::Char('l') => (None as Option<Event>).maybe_update(self.select(1, cap)),
          _ => Some(event.into()),
        }
      },
      Event::Custom(data) => {
        match data.downcast::<TermUiEvent>() {
          Ok(e) => self.handle_custom_event(e, cap),
          Err(e) => panic!("Received unexpected custom event: {:?}", e),
        }
      },
    }
  }
}
