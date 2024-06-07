// Copyright (C) 2018-2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::future::Future;
use std::ops::Deref as _;
use std::pin::Pin;

use async_trait::async_trait;

use gui::derive::Widget;
use gui::Cap;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::Widget;

use super::event::Event;
use super::input::InputResult;
use super::input::InputText;
use super::message::Message;
use super::message::MessageExt;
use super::modal::Modal;


/// An object representing the in/out area within the `TermUi`.
#[derive(Debug)]
pub enum InOut {
  Saved,
  Search(String),
  Error(String),
  Input(InputText),
  Clear,
}

impl PartialEq for InOut {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (InOut::Saved, InOut::Saved) => true,
      (InOut::Search(x), InOut::Search(y)) => x == y,
      (InOut::Error(x), InOut::Error(y)) => x == y,
      (InOut::Input(x), InOut::Input(y)) => x.deref() == y.deref(),
      (InOut::Clear, InOut::Clear) => true,
      _ => false,
    }
  }
}

impl Eq for InOut {}


#[derive(Debug)]
struct InOutState {
  /// The actual `InOut` state we have.
  in_out: InOut,
  /// The generation ID. The ID is incremented on every change being
  /// made to `in_out`.
  gen: usize,
}

impl InOutState {
  /// Retrieve the current `InOut` state.
  fn get(&self) -> &InOut {
    &self.in_out
  }

  /// Retrieve a mutable reference to the current `InOut` state.
  ///
  /// # Notes
  /// Be careful to bump the state's generation ID as necessary if
  /// making modifications.
  fn get_mut(&mut self) -> &mut InOut {
    &mut self.in_out
  }

  /// Update the current `InOut` state.
  fn set(&mut self, in_out: InOut) {
    self.in_out = in_out;
    self.bump()
  }

  /// Bump the generation ID.
  fn bump(&mut self) {
    self.gen += 1;
  }
}

impl Default for InOutState {
  fn default() -> Self {
    Self {
      in_out: InOut::Clear,
      gen: 0,
    }
  }
}


/// The data associated with an `InOutArea`.
pub struct InOutAreaData {
  /// The ID of the widget that was focused before the input/output area
  /// received the input focus.
  prev_focused: Option<Id>,
  /// The generation number at which to clear the state.
  clear_gen: Option<usize>,
  /// The state of the area.
  in_out: InOutState,
}

impl InOutAreaData {
  /// Create a new `InOutAreaData` object.
  pub fn new() -> Self {
    Self {
      prev_focused: None,
      clear_gen: None,
      in_out: Default::default(),
    }
  }

  /// Conditionally change the `InOut` state of the widget.
  ///
  /// # Notes
  /// `parent` should be the `Id` of the parent widget. We should make
  /// sure to always update the parent widget (as opposed to the `InOut`
  /// widget itself), because it may want to set the cursor in response
  /// to this widget being hidden, for example.
  fn change_state(&mut self, parent: Id, in_out: Option<InOut>) -> Option<Message> {
    // We received a request to change the state. Unconditionally bump
    // the generation it has, irrespective of whether we actually change
    // it (which we don't, if the new state is equal to what we already
    // have).
    self.in_out.bump();

    match in_out {
      Some(in_out) if in_out != *self.in_out.get() => {
        self.in_out.set(in_out);
        Some(Message::updated(parent))
      },
      Some(..) => None,
      None => Some(Message::updated(parent)),
    }
  }
}


/// A widget representing an input/output and status area.
#[derive(Debug, Widget)]
#[gui(Event = Event, Message = Message)]
pub struct InOutArea {
  id: Id,
}

impl InOutArea {
  /// Create a new input/output area object.
  pub fn new(id: Id, cap: &mut dyn MutCap<Event, Message>) -> Self {
    // Install a hook to be able to reset the input/output area into
    // "clear" state on every key press.
    cap.hook_events(id, Some(&InOutArea::handle_hooked_event));
    Self { id }
  }

  /// Handle a hooked event.
  fn handle_hooked_event<'f>(
    widget: &'f dyn Widget<Event, Message>,
    cap: &'f mut dyn MutCap<Event, Message>,
    event: Option<&'f Event>,
  ) -> Pin<Box<dyn Future<Output = Option<Event>> + 'f>> {
    Box::pin(async move {
      // SANITY: We know that this dialog has a parent.
      let parent = cap.parent_id(widget.id()).unwrap();
      let data = cap
        .data_mut(widget.id())
        .downcast_mut::<InOutAreaData>()
        .unwrap();
      if let Some(event) = event {
        // We remember the generation number we had when we entered the
        // pre-hook such that we can decide whether to set our state to
        // "Clear" or not on the post-hook path.
        match event {
          Event::Key(..) => {
            data.clear_gen = Some(data.in_out.gen);
            None
          },
          Event::Updated(..) | Event::Quit => None,
        }
      } else {
        // We only change our state to "Clear" if the generation number
        // is still the same, meaning that we did not change our state
        // between pre- and post-hook.
        if data.clear_gen.take() == Some(data.in_out.gen) {
          match data.in_out.get() {
            InOut::Saved | InOut::Search(_) | InOut::Error(_) => {
              data.change_state(parent, Some(InOut::Clear)).into_event()
            },
            InOut::Input(..) | InOut::Clear => None,
          }
        } else {
          None
        }
      }
    })
  }

  /// Finish text input by changing the internal state and emitting an event.
  async fn finish_input(
    &self,
    cap: &mut dyn MutCap<Event, Message>,
    string: Option<String>,
  ) -> Option<Message> {
    // SANITY: We know that this dialog has a parent.
    let parent = cap.parent_id(self.id).unwrap();
    let data = self.data_mut::<InOutAreaData>(cap);
    let result1 = data.change_state(parent, Some(InOut::Clear));
    let widget = self.restore_focus(cap);
    let message = if let Some(s) = string {
      Message::EnteredText(s)
    } else {
      Message::InputCanceled
    };

    let result2 = cap.send(widget, message).await;
    result1.maybe_update(result2)
  }

  /// Retrieve the input/output area's current state.
  pub fn state<'slf>(&'slf self, cap: &'slf dyn Cap) -> &'slf InOut {
    let data = self.data::<InOutAreaData>(cap);
    data.in_out.get()
  }
}

impl Modal for InOutArea {
  fn prev_focused(&self, cap: &dyn Cap) -> Option<Id> {
    self.data::<InOutAreaData>(cap).prev_focused
  }

  fn set_prev_focused(&self, cap: &mut dyn MutCap<Event, Message>, focused: Option<Id>) {
    let data = self.data_mut::<InOutAreaData>(cap);
    data.prev_focused = focused;
  }
}

#[async_trait(?Send)]
impl Handleable<Event, Message> for InOutArea {
  /// Handle an event.
  async fn handle(&self, cap: &mut dyn MutCap<Event, Message>, event: Event) -> Option<Event> {
    match event {
      Event::Key(key, raw) => {
        // SANITY: We know that this dialog has a parent.
        let parent = cap.parent_id(self.id).unwrap();
        let data = self.data_mut::<InOutAreaData>(cap);
        let text = if let InOut::Input(text) = data.in_out.get_mut() {
          text
        } else {
          panic!("In/out area not used for input.");
        };

        let message = match text.handle_key(key, &raw) {
          InputResult::Completed(text) => self.finish_input(cap, Some(text)).await,
          InputResult::Canceled => self.finish_input(cap, None).await,
          InputResult::Updated => data.change_state(parent, None),
          InputResult::Unchanged => {
            data.in_out.bump();
            None
          },
        };

        message.into_event()
      },
      _ => Some(event),
    }
  }

  /// React to a message.
  async fn react(&self, message: Message, cap: &mut dyn MutCap<Event, Message>) -> Option<Message> {
    match message {
      Message::SetInOut(in_out) => {
        if matches!(in_out, InOut::Input(..)) {
          self.make_focused(cap);
        };

        // SANITY: We know that this dialog has a parent.
        let parent = cap.parent_id(self.id).unwrap();
        let data = self.data_mut::<InOutAreaData>(cap);
        data.change_state(parent, Some(in_out))
      },
      #[cfg(all(test, not(feature = "readline")))]
      Message::GetInOut => {
        use crate::text::EditableText;

        let data = self.data::<InOutAreaData>(cap);

        // A poor man's Clone impl for `InOut`. We don't really want to
        // implement Clone for `InOut` because it contains a `Readline`
        // instance and we try to avoid copying those if at all
        // possible.
        // In this instance, we are on the !readline path anyway, so it
        // doesn't really matter.
        let in_out = match data.in_out.get() {
          InOut::Saved => InOut::Saved,
          InOut::Search(x) => InOut::Search(x.clone()),
          InOut::Error(x) => InOut::Error(x.clone()),
          InOut::Input(x) => InOut::Input(InputText::new(EditableText::clone(x.deref()))),
          InOut::Clear => InOut::Clear,
        };
        Some(Message::GotInOut(in_out))
      },
      message => panic!("Received unexpected message: {message:?}"),
    }
  }
}
