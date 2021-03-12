// *************************************************************************
// * Copyright (C) 2018,2021 Daniel Mueller (deso@posteo.net)              *
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

pub use uid::Id;

use crate::ser::id::Id as SerId;
use crate::ser::ToSerde;


impl<T, U> ToSerde<SerId<U>> for Id<T>
where
  T: Copy,
  U: Copy,
{
  /// Convert this `Id` into a serializable one.
  ///
  /// Note that it is generally safe to convert this unique in-memory ID
  /// into a serializable one. However, the inverse conversion is not
  /// allowed, for there is no way to guarantee uniqueness of the
  /// resulting in-memory ID.
  fn to_serde(&self) -> SerId<U> {
    SerId::new(self.get())
  }
}
