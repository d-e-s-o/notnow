// Copyright (C) 2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Debug;

use async_trait::async_trait;

use gui::derive::Widget;
use gui::Cap;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::Widget;

use super::event::Event;
use super::event::KeyEvent;
use super::message::Message;
use super::message::MessageExt as _;
use super::modal::Modal;


#[derive(Debug, Default)]
pub struct KseqData {
  /// The ID of the widget that was focused beforehand.
  prev_focused: Option<Id>,
  /// The key pressed to initiate the key sequence.
  seq_key: Option<KeyEvent>,
  /// The ID of the widget that installed the key sequence hook.
  response_id: Option<Id>,
}


/// A widget used for capturing a sequence of key presses.
#[derive(Debug, Widget)]
#[gui(Event = Event, Message = Message)]
pub struct Kseq {
  id: Id,
}

impl Kseq {
  pub fn new(id: Id, cap: &mut dyn MutCap<Event, Message>) -> Self {
    let slf = Self { id };
    let _data = slf.data_mut::<KseqData>(cap);
    slf
  }
}

impl Modal for Kseq {
  fn prev_focused(&self, cap: &dyn Cap) -> Option<Id> {
    self.data::<KseqData>(cap).prev_focused
  }

  fn set_prev_focused(&self, cap: &mut dyn MutCap<Event, Message>, focused: Option<Id>) {
    let data = self.data_mut::<KseqData>(cap);
    data.prev_focused = focused;
  }
}

#[async_trait(?Send)]
impl Handleable<Event, Message> for Kseq {
  /// Handle an event.
  async fn handle(&self, cap: &mut dyn MutCap<Event, Message>, event: Event) -> Option<Event> {
    // We are sure that we are done here, so always make sure to restore
    // the input focus before potentially interacting with other
    // widgets.
    let focused = self.restore_focus(cap);

    let data = self.data_mut::<KseqData>(cap);
    // SANITY: We always ensure a `seq_key` is set before
    //         setting up an event hook.
    let seq_key = data.seq_key.take().unwrap();
    // SANITY: We always ensure a `response_id` is set before
    //         setting up an event hook.
    let response_id = data.response_id.take().unwrap();

    let msg = match event {
      Event::Key(key_event) => {
        let msg = Message::GotKeySeq(seq_key, key_event);
        cap.send(response_id, msg).await
      },
      // SANITY: We shouldn't receive anything but a key press if for no
      //         other reason than that all other `Event` variants are
      //         only meant as output.
      _ => unreachable!(),
    };

    // If the widget did not consume the key press, let the now-focused
    // widget deal with it.
    if let Some(Message::UnhandledKey(key)) = msg {
      // Note that `focused` does not necessarily have to actually be
      // focused anymore at this point (it could have been changed in
      // response to the message sent above). But it's up for debate
      // which widget to forward the event to. We can revisit this
      // decision should it turn out to be wrong.
      cap.rehandle(focused, Event::Key(key)).await
    } else {
      msg.into_event()
    }
  }

  /// React to a message.
  async fn react(&self, message: Message, cap: &mut dyn MutCap<Event, Message>) -> Option<Message> {
    match message {
      Message::StartKeySeq(response_id, key) => {
        let () = self.make_focused(cap);

        let data = self.data_mut::<KseqData>(cap);
        data.response_id = Some(response_id);
        data.seq_key = Some(key);
        None
      },
      message => panic!("Received unexpected message: {message:?}"),
    }
  }
}
