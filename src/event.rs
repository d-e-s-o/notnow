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

use termion::event::Key as TermKey;

use gui::ChainEvent as GuiChainEvent;
use gui::EventChain;
use gui::Key as GuiKey;
use gui::UiEvent as GuiUiEvent;
use gui::UiEvents as GuiUiEvents;

use crate::termui::TermUiEvent;


pub trait EventUpdated {
  /// Check whether the event has been updated.
  fn is_updated(&self) -> bool;
}

impl EventUpdated for GuiUiEvent {
  fn is_updated(&self) -> bool {
    match self {
      GuiUiEvent::Custom(data) |
      GuiUiEvent::Directed(_, data) |
      GuiUiEvent::Returnable(_, _, data) => {
        if let Some(event) = data.downcast_ref::<TermUiEvent>() {
          event.is_updated()
        } else {
          false
        }
      },
      _ => false,
    }
  }
}

impl EventUpdated for GuiUiEvents {
  fn is_updated(&self) -> bool {
    match self {
      GuiChainEvent::Event(event) => event.is_updated(),
      GuiChainEvent::Chain(event, chain) => event.is_updated() || chain.is_updated(),
    }
  }
}


/// A trait to chain a `TermUiEvent::Updated` event to an event.
pub trait EventUpdate {
  /// Chain an update event onto yourself.
  fn update(self) -> Option<GuiUiEvents>;

  /// Potentially chain an update event onto yourself.
  fn maybe_update(self, update: bool) -> Option<GuiUiEvents>;
}

impl<E> EventUpdate for Option<E>
where
  E: Into<GuiUiEvents>,
{
  fn update(self) -> Option<GuiUiEvents> {
    let updated = GuiUiEvent::Custom(Box::new(TermUiEvent::Updated));

    Some(match self {
      Some(event) => {
        let event = event.into();
        // Redrawing everything is expensive (and we do that for every
        // `Updated` event we encounter), so make sure that we only ever
        // have one.
        if !event.is_updated() {
          event.chain(updated)
        } else {
          event
        }
      },
      None => updated.into(),
    })
  }

  fn maybe_update(self, update: bool) -> Option<GuiUiEvents> {
    if update {
      self.update()
    } else {
      self.and_then(|x| Some(x.into()))
    }
  }
}


/// Convert a `termion::event::Key` into a `gui::Key`.
///
/// If the conversion fails, the supplied key is returned.
pub fn convert(key: TermKey) -> Result<GuiKey, TermKey> {
  match key {
    TermKey::Backspace => Ok(GuiKey::Backspace),
    TermKey::Char(c) if c == '\n' => Ok(GuiKey::Return),
    TermKey::Char(c) => Ok(GuiKey::Char(c)),
    TermKey::Delete => Ok(GuiKey::Delete),
    TermKey::Down => Ok(GuiKey::Down),
    TermKey::End => Ok(GuiKey::End),
    TermKey::Esc => Ok(GuiKey::Esc),
    TermKey::Home => Ok(GuiKey::Home),
    TermKey::Insert => Ok(GuiKey::Insert),
    TermKey::Left => Ok(GuiKey::Left),
    TermKey::PageDown => Ok(GuiKey::PageDown),
    TermKey::PageUp => Ok(GuiKey::PageUp),
    TermKey::Right => Ok(GuiKey::Right),
    TermKey::Up => Ok(GuiKey::Up),
    _ => Err(key),
  }
}


#[cfg(test)]
pub mod tests {
  use super::*;

  use gui::ChainEvent;
  use gui::Event;
  use gui::Key;
  use gui::UiEvent;
  use gui::UnhandledEvent;
  use gui::UnhandledEvents;


  /// A trait for working with custom events.
  pub trait CustomEvent {
    /// Unwrap a custom event of type `T`.
    fn unwrap_custom<T>(self) -> T
    where
      T: 'static;
  }

  impl CustomEvent for UnhandledEvent {
    fn unwrap_custom<T>(self) -> T
    where
      T: 'static,
    {
      match self {
        UnhandledEvent::Custom(event) => *event.downcast::<T>().unwrap(),
        _ => panic!("Unexpected event: {:?}", self),
      }
    }
  }

  impl CustomEvent for UnhandledEvents {
    fn unwrap_custom<T>(self) -> T
    where
      T: 'static,
    {
      match self {
        ChainEvent::Event(event) => event.unwrap_custom(),
        _ => panic!("Unexpected event: {:?}", self),
      }
    }
  }


  #[test]
  fn update_none() {
    let event = (None as Option<Event>).update().update();
    match event.unwrap() {
      ChainEvent::Event(event) => {
        match event {
          UiEvent::Custom(data) => {
            let event = data.downcast::<TermUiEvent>().unwrap();
            assert!(event.is_updated())
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
      ChainEvent::Chain(event, chain) => {
        match event {
          UiEvent::Event(event) => {
            match event {
              Event::KeyUp(key) => assert_eq!(key, Key::Char(' ')),
              _ => assert!(false),
            }
          },
          _ => assert!(false),
        };

        match *chain {
          ChainEvent::Event(event) => {
            match event {
              UiEvent::Custom(data) => {
                let event = data.downcast::<TermUiEvent>().unwrap();
                assert!(event.is_updated())
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
  fn unwrap_unhandled_event() {
    let event = UnhandledEvent::Custom(Box::new(42u64));
    let event = ChainEvent::Event(event);

    assert_eq!(event.unwrap_custom::<u64>(), 42);
  }

  #[test]
  #[should_panic(expected = "Unexpected event")]
  fn unwrap_unhandled_event_of_wrong_type() {
    let event = Event::KeyUp(Key::Esc);
    let event = UnhandledEvent::Event(event);
    let event = ChainEvent::Event(event);

    event.unwrap_custom::<u64>();
  }
}
