// view.rs

// *************************************************************************
// * Copyright (C) 2017-2018 Daniel Mueller (deso@posteo.net)              *
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

use std::io::Error;
use std::result;

use termion::event::Event;


/// A result type as used by every view.
///
/// TODO: For now we assume a view uses io::Error. This may change should
///       we ever support more views.
pub type Result<T> = result::Result<T, Error>;


/// An enum to indicate whether the application should exit.
pub enum Quit {
  Yes,
  No,
}


/// A trait defining the interface every view needs to implement.
///
/// Views are the entities representing the actual data to the user.
pub trait View {
  /// Check for and handle any new input on the view.
  fn handle(&mut self, event: &Event) -> Result<Quit>;

  /// Render the view to reflect new data.
  fn render(&mut self) -> Result<()>;
}
