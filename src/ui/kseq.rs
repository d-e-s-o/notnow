// Copyright (C) 2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Debug;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;

use gui::derive::Widget;
use gui::EventHookFn;
use gui::Handleable;
use gui::Id;
use gui::MutCap;
use gui::Widget;

use termion::event::Key;

use super::event::Event;
use super::message::Message;


/// A type providing a derive for `Debug` for types that
/// otherwise don't.
#[derive(Clone, Copy)]
struct D<T>(T);

impl<T> Debug for D<T> {
  fn fmt(&self, fmt: &mut Formatter<'_>) -> FmtResult {
    write!(fmt, "{:?}", &self.0 as *const T)
  }
}


#[derive(Debug, Default)]
pub struct KseqData {
  /// The key pressed to initiate the key sequence.
  seq_key: Option<Key>,
  /// The ID of the widget that installed the key sequence hook.
  hooker_id: Option<Id>,
  /// The event hook that was previously active.
  hook_fn: Option<D<EventHookFn<Event, Message>>>,
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

  /// Handle a hooked event.
  fn handle_hooked_event<'f>(
    widget: &'f dyn Widget<Event, Message>,
    cap: &'f mut dyn MutCap<Event, Message>,
    event: Option<&'f Event>,
  ) -> Pin<Box<dyn Future<Output = Option<Event>> + 'f>> {
    Box::pin(async move {
      if let Some(event) = event {
        if let Event::Key(key, ..) = event {
          let data = cap
            .data_mut(widget.id())
            .downcast_mut::<KseqData>()
            .unwrap();
          // SANITY: We always ensure a `seq_key` is set before
          //         setting up an event hook.
          let seq_key = data.seq_key.unwrap();
          // SANITY: We always ensure a `hooker_id` is set before
          //         setting up an event hook.
          let hooker_id = data.hooker_id.unwrap();
          let msg = Message::GotKeySeq(seq_key, *key);
          let _msg = cap.send(hooker_id, msg).await;
        }

        let data = cap
          .data_mut(widget.id())
          .downcast_mut::<KseqData>()
          .unwrap();
        let _seq_key = data.seq_key.take();
        let _hooker_id = data.hooker_id.take();
        let hook_fn = data.hook_fn.take().map(|D(h)| h);
        let _hook_fn = cap.hook_events(widget.id(), hook_fn);
      }
      None
    })
  }
}

#[async_trait(?Send)]
impl Handleable<Event, Message> for Kseq {
  /// React to a message.
  async fn react(&self, message: Message, cap: &mut dyn MutCap<Event, Message>) -> Option<Message> {
    match message {
      Message::StartKeySeq(hooker_id, key) => {
        let hook_fn = cap.hook_events(self.id, Some(&Self::handle_hooked_event));
        let data = self.data_mut::<KseqData>(cap);
        data.hooker_id = Some(hooker_id);
        data.seq_key = Some(key);
        data.hook_fn = hook_fn.map(D);
        None
      },
      message => panic!("Received unexpected message: {message:?}"),
    }
  }
}
