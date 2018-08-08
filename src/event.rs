// event.rs

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

use termion::event::Event as TermEvent;
use termion::event::Key as TermKey;

use gui::Event as GuiEvent;
use gui::EventChain;
use gui::Key as GuiKey;
use gui::MetaEvent as GuiMetaEvent;
use gui::UiEvent as GuiUiEvent;

use termui::TermUiEvent;


fn is_event_updated(event: &GuiEvent) -> bool {
  match event {
    GuiEvent::Custom(data) => {
      if let Some(event) = data.downcast_ref::<TermUiEvent>() {
        event.is_updated()
      } else {
        false
      }
    },
    _ => false,
  }
}

fn is_ui_event_updated(event: &GuiUiEvent) -> bool {
  match event {
    GuiUiEvent::Event(event) => is_event_updated(event),
    _ => false,
  }
}

/// Check whether `update` has ever been called on the given event.
pub fn is_updated(event: &GuiMetaEvent) -> bool {
  match event {
    GuiMetaEvent::UiEvent(event) => is_ui_event_updated(event),
    GuiMetaEvent::Chain(event, meta_event) => {
      is_ui_event_updated(event) || is_updated(meta_event)
    },
  }
}


/// A trait to chain a `TermUiEvent::Updated` event to an event.
pub trait EventUpdated {
  /// Chain an update event onto yourself.
  fn update(self) -> Option<GuiMetaEvent>;

  /// Potentially chain an update event onto yourself.
  fn maybe_update(self, update: bool) -> Option<GuiMetaEvent>;
}

impl<E> EventUpdated for Option<E>
where
  E: Into<GuiMetaEvent>,
{
  fn update(self) -> Option<GuiMetaEvent> {
    let updated = GuiEvent::Custom(Box::new(TermUiEvent::Updated));

    Some(match self {
      Some(event) => {
        let event = event.into();
        // Redrawing everything is expensive (and we do that for every
        // `Updated` event we encounter), so make sure that we only ever
        // have one.
        if !is_updated(&event) {
          event.chain(updated)
        } else {
          event
        }
      },
      None => updated.into(),
    })
  }

  fn maybe_update(self, update: bool) -> Option<GuiMetaEvent> {
    if update {
      self.update()
    } else {
      self.and_then(|x| Some(x.into()))
    }
  }
}


/// Convert a `termion::event::Event` into a `gui::Event`.
///
/// If the conversion fails, the original event is returned.
pub fn convert(event: TermEvent) -> Result<GuiEvent, TermEvent> {
  match event {
    TermEvent::Key(key) => {
      match key {
        TermKey::Backspace => Ok(GuiEvent::KeyDown(GuiKey::Backspace)),
        TermKey::Char(c) if c == '\n' => Ok(GuiEvent::KeyDown(GuiKey::Return)),
        TermKey::Char(c) => Ok(GuiEvent::KeyDown(GuiKey::Char(c))),
        TermKey::Delete => Ok(GuiEvent::KeyDown(GuiKey::Delete)),
        TermKey::Down => Ok(GuiEvent::KeyDown(GuiKey::Down)),
        TermKey::End => Ok(GuiEvent::KeyDown(GuiKey::End)),
        TermKey::Esc => Ok(GuiEvent::KeyDown(GuiKey::Esc)),
        TermKey::Home => Ok(GuiEvent::KeyDown(GuiKey::Home)),
        TermKey::Insert => Ok(GuiEvent::KeyDown(GuiKey::Insert)),
        TermKey::Left => Ok(GuiEvent::KeyDown(GuiKey::Left)),
        TermKey::PageDown => Ok(GuiEvent::KeyDown(GuiKey::PageDown)),
        TermKey::PageUp => Ok(GuiEvent::KeyDown(GuiKey::PageUp)),
        TermKey::Right => Ok(GuiEvent::KeyDown(GuiKey::Right)),
        TermKey::Up => Ok(GuiEvent::KeyDown(GuiKey::Up)),
        _ => Err(event),
      }
    },
    _ => Err(event),
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;

  use gui::Event;
  use gui::Key;
  use gui::MetaEvent;
  use gui::UiEvent;


  /// A trait for working with custom events.
  pub trait CustomEvent {
    /// Unwrap a custom event of type `T`.
    fn unwrap_custom<T>(self) -> T
    where
      T: 'static;
  }

  impl CustomEvent for Event {
    fn unwrap_custom<T>(self) -> T
    where
      T: 'static,
    {
      match self {
        Event::Custom(x) => *x.downcast::<T>().unwrap(),
        _ => panic!("Unexpected event: {:?}", self),
      }
    }
  }

  impl CustomEvent for UiEvent {
    fn unwrap_custom<T>(self) -> T
    where
      T: 'static,
    {
      match self {
        UiEvent::Event(event) => event.unwrap_custom(),
        _ => panic!("Unexpected event: {:?}", self),
      }
    }
  }

  impl CustomEvent for MetaEvent {
    fn unwrap_custom<T>(self) -> T
    where
      T: 'static,
    {
      match self {
        MetaEvent::UiEvent(event) => event.unwrap_custom(),
        _ => panic!("Unexpected event: {:?}", self),
      }
    }
  }


  #[test]
  fn update_none() {
    let event = (None as Option<Event>).update().update();
    match event.unwrap() {
      MetaEvent::UiEvent(event) => {
        match event {
          UiEvent::Event(event) => {
            match event {
              Event::Custom(data) => {
                let event = data.downcast::<TermUiEvent>().unwrap();
                assert!(event.is_updated())
              },
              _ => assert!(false),
            }
          },
          _ => assert!(false),
        }
      },
      _ => assert!(false),
    }
  }

  #[test]
  fn update_some_event() {
    let event = Some(Event::KeyUp(Key::Char(' '))).update().update();
    match event.unwrap() {
      MetaEvent::Chain(event, meta_event) => {
        match event {
          UiEvent::Event(event) => {
            match event {
              Event::KeyUp(key) => assert_eq!(key, Key::Char(' ')),
              _ => assert!(false),
            }
          },
          _ => assert!(false),
        };

        match *meta_event {
          MetaEvent::UiEvent(event) => {
            match event {
              UiEvent::Event(event) => {
                match event {
                  Event::Custom(data) => {
                    let event = data.downcast::<TermUiEvent>().unwrap();
                    assert!(event.is_updated())
                  },
                  _ => assert!(false),
                }
              },
              _ => assert!(false),
            }
          },
          _ => assert!(false),
        };
      },
      _ => assert!(false),
    }
  }

  #[test]
  fn unwrap_meta_event() {
    let event = Event::Custom(Box::new(42u64));
    let event = UiEvent::Event(event);
    let event = MetaEvent::UiEvent(event);

    assert_eq!(event.unwrap_custom::<u64>(), 42);
  }

  #[test]
  #[should_panic(expected = "Unexpected event")]
  fn unwrap_meta_event_of_wrong_type() {
    let event = Event::KeyUp(Key::Esc);
    let event = UiEvent::Event(event);
    let event = MetaEvent::UiEvent(event);

    event.unwrap_custom::<u64>();
  }
}
