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

use termui::TermUiEvent;


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
      Some(event) => event.chain(updated),
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


  #[test]
  fn update_none() {
    let event = (None as Option<Event>).update();
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
    let event = Some(Event::KeyUp(Key::Char(' '))).update();
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
}
