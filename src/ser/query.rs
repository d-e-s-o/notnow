// query.rs

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

use ser::tags::Tag;


/// A query that can be serialized and deserialized.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Query {
  pub name: String,
  pub tags: Vec<Vec<Tag>>,
}


#[cfg(test)]
mod tests {
  use super::*;

  use serde_json::from_str as from_json;
  use serde_json::to_string as to_json;

  use ser::id::Id;


  #[test]
  fn serialize_deserialize_query() {
    let tag1 = Tag {
      id: Id::new(1),
    };
    let tag2 = Tag {
      id: Id::new(2),
    };
    let tag3 = Tag {
      id: Id::new(3),
    };
    let tag4 = Tag {
      id: Id::new(4),
    };

    let query = Query {
      name: "test-query".to_string(),
      tags: vec![
        vec![tag1],
        vec![tag2, tag3],
        vec![tag4, tag2],
      ],
    };

    let serialized = to_json(&query).unwrap();
    let deserialized = from_json::<Query>(&serialized).unwrap();

    assert_eq!(deserialized, query);
  }
}
