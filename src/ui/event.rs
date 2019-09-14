// event.rs

// *************************************************************************
// * Copyright (C) 2018-2019 Daniel Mueller (deso@posteo.net)              *
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

use gui::ChainEvent;
use gui::EventChain;
use gui::UiEvent;
use gui::UiEvents;
use gui::UnhandledEvent;
use gui::UnhandledEvents;

use super::termui::TermUiEvent;

/// A key as used by the UI.
pub use termion::event::Key;


/// An event as used by the UI.
#[derive(Clone, Debug)]
pub enum Event {
  #[cfg(not(feature = "readline"))]
  Key(Key, ()),
  #[cfg(feature = "readline")]
  Key(Key, Vec<u8>),
}

impl From<u8> for Event {
  fn from(b: u8) -> Self {
    #[cfg(not(feature = "readline"))]
    { Event::Key(Key::Char(char::from(b)), ()) }
    #[cfg(feature = "readline")]
    { Event::Key(Key::Char(char::from(b)), vec![b]) }
  }
}


pub trait EventUpdated {
  /// Check whether the event has been updated.
  fn is_updated(&self) -> bool;
}

impl EventUpdated for UiEvent<Event> {
  fn is_updated(&self) -> bool {
    match self {
      UiEvent::Custom(data) |
      UiEvent::Directed(_, data) |
      UiEvent::Returnable(_, _, data) => {
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

impl EventUpdated for UiEvents<Event> {
  fn is_updated(&self) -> bool {
    match self {
      ChainEvent::Event(event) => event.is_updated(),
      ChainEvent::Chain(event, chain) => event.is_updated() || chain.is_updated(),
    }
  }
}

impl EventUpdated for UnhandledEvent<Event> {
  fn is_updated(&self) -> bool {
    match self {
      UnhandledEvent::Custom(data) => {
        if let Some(event) = data.downcast_ref::<TermUiEvent>() {
          event.is_updated()
        } else {
          false
        }
      },
      UnhandledEvent::Event(_) |
      UnhandledEvent::Quit => false,
    }
  }
}

impl EventUpdated for UnhandledEvents<Event> {
  fn is_updated(&self) -> bool {
    match self {
      ChainEvent::Event(event) => event.is_updated(),
      ChainEvent::Chain(event, chain) => event.is_updated() || chain.is_updated(),
    }
  }
}


/// A trait to chain a `TermUiEvent::Updated` event to an event.
pub trait EventUpdate {
  /// Chain an update event onto yourself.
  fn update(self) -> Option<UiEvents<Event>>;

  /// Potentially chain an update event onto yourself.
  fn maybe_update(self, update: bool) -> Option<UiEvents<Event>>;
}

impl<E> EventUpdate for Option<E>
where
  E: Into<UiEvents<Event>>,
{
  fn update(self) -> Option<UiEvents<Event>> {
    let updated = UiEvent::Custom(Box::new(TermUiEvent::Updated));

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

  fn maybe_update(self, update: bool) -> Option<UiEvents<Event>> {
    if update {
      self.update()
    } else {
      self.and_then(|x| Some(x.into()))
    }
  }
}


#[allow(unused_results)]
#[cfg(test)]
pub mod tests {
  use super::*;

  use gui::ChainEvent;
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

  impl CustomEvent for UnhandledEvent<Event> {
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

  impl CustomEvent for UnhandledEvents<Event> {
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
          _ => panic!(),
        }
      },
      _ => panic!(),
    }
  }

  #[test]
  fn update_some_event() {
    let event = Some(Event::from(b' ')).update().update();
    match event.unwrap() {
      ChainEvent::Chain(event, chain) => {
        match event {
          UiEvent::Event(Event::Key(key, _)) => {
            assert_eq!(key, Key::Char(' '))
          },
          _ => panic!(),
        };

        match *chain {
          ChainEvent::Event(event) => {
            match event {
              UiEvent::Custom(data) => {
                let event = data.downcast::<TermUiEvent>().unwrap();
                assert!(event.is_updated())
              },
              _ => panic!(),
            }
          },
          _ => panic!(),
        };
      },
      _ => panic!(),
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
    let event = Event::from(b'x');
    let event = UnhandledEvent::Event(event);
    let event = ChainEvent::Event(event);

    event.unwrap_custom::<u64>();
  }
}
