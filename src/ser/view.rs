// Copyright (C) 2018-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module providing serialization and deserialization support for
//! task views.

use std::fmt::Debug;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::num::NonZeroUsize;

use serde::Deserialize;
use serde::Serialize;

use crate::formula::Formula;
use crate::ser::tags::Id;
use crate::ser::tags::Tag;


/// A literal that can be serialized and deserialized.
#[derive(Copy, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TagLit {
  Pos(Tag),
  Neg(Tag),
}

impl Debug for TagLit {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    match self {
      Self::Pos(tag) => write!(f, "{tag}"),
      Self::Neg(tag) => write!(f, "!{tag}"),
    }
  }
}

impl TagLit {
  /// Retrieve the ID of the wrapped tag.
  pub fn id(&self) -> Id {
    match self {
      TagLit::Pos(tag) | TagLit::Neg(tag) => tag.id,
    }
  }
}

impl From<&TagLit> for Formula {
  fn from(other: &TagLit) -> Self {
    match other {
      TagLit::Pos(tag) => Formula::Var(tag.id.get()),
      TagLit::Neg(tag) => !Formula::Var(tag.id.get()),
    }
  }
}


/// A view that can be serialized and deserialized.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct View {
  pub name: String,
  pub lits: Box<[Box<[TagLit]>]>,
}


/// Convert a formula in CNF form into a [`Formula`] object.
pub(crate) fn cnf_to_formula<T>(cnf: &[Box<[T]>]) -> Option<Formula>
where
  for<'t> Formula: From<&'t T>,
{
  let mut formula = None;

  for ors in cnf.iter().rev() {
    let mut it = ors.iter().rev().map(Formula::from);
    let mut ors = if let Some(or) = it.next() {
      or
    } else {
      continue
    };

    ors = it.fold(ors, |ors, or| or | ors);

    formula = if let Some(formula) = formula {
      Some(ors & formula)
    } else {
      Some(ors)
    };
  }

  formula
}


/// Convert a formula into an equivalent CNF form.
///
/// If a tag with value zero is encountered, `None` will be returned.
pub(crate) fn formula_to_cnf(formula: Formula) -> Option<Box<[Box<[TagLit]>]>> {
  fn rewrite(formula: Formula) -> Option<Vec<Vec<TagLit>>> {
    match formula {
      Formula::Var(a) => Some(vec![vec![TagLit::Pos(Tag::from(Id::new(
        NonZeroUsize::new(a)?,
      )))]]),
      Formula::Not(a) => match *a {
        Formula::Var(b) => Some(vec![vec![TagLit::Neg(Tag::from(Id::new(
          NonZeroUsize::new(b)?,
        )))]]),
        // Remove double negation.
        Formula::Not(b) => rewrite(*b),
        // De Morgan: !(b & c) -> (!b | !c)
        Formula::And(b, c) => rewrite(!b | !c),
        // De Morgan: !(b | c) -> (!b & !c)
        Formula::Or(b, c) => rewrite(!b & !c),
      },
      Formula::And(a, b) => {
        let mut a = rewrite(*a)?;
        let b = rewrite(*b)?;
        let () = a.extend(b);
        Some(a)
      },
      Formula::Or(a, b) => {
        // rewrite(A) must have the form A1 ^ A2 ^ ... ^ Am, and
        // rewrite(B) must have the form B1 ^ B2 ^ ... ^ Bn,
        // where all the Ai and Bi are dijunctions of literals.
        // So we need a CNF formula equivalent to
        //    (A1 ^ A2 ^ ... ^ Am) v (B1 ^ B2 ^ ... ^ Bn).
        // So return (A1 v B1) ^ (A1 v B2) ^ ... ^ (A1 v Bn)
        //         ^ (A2 v B1) ^ (A2 v B2) ^ ... ^ (A2 v Bn)
        //           ...
        //         ^ (Am v B1) ^ (Am v B2) ^ ... ^ (Am v Bn)

        // TODO: We risk exponential blow up here. Different algorithms
        //       may do better. Tseytin transformation would guarantee
        //       size linear with respect to the input formula.
        let a = rewrite(*a)?;
        let b = rewrite(*b)?;

        let mut c = Vec::new();
        for ax in &a {
          for bx in &b {
            let conjunctions = ax
              .iter()
              .copied()
              .chain(bx.iter().copied())
              .collect::<Vec<_>>();
            let () = c.push(conjunctions);
          }
        }
        Some(c)
      },
    }
  }

  let cnf = rewrite(formula)?
    .into_iter()
    .map(|vec| vec.into_boxed_slice())
    .collect::<Box<[_]>>();
  Some(cnf)
}


#[cfg(test)]
mod tests {
  use super::*;

  use std::str::FromStr as _;

  use crate::ser::backends::Backend;
  use crate::ser::backends::Json;

  use crate::ser::id::Id;

  fn var(var: usize) -> Formula {
    Formula::Var(var)
  }

  fn tag(tag: usize) -> Tag {
    Tag {
      id: Id::try_from(tag).unwrap(),
    }
  }


  /// Check that we can serialize and deserialize a `View`.
  #[test]
  fn serialize_deserialize_view() {
    let view = View {
      name: "test-view".to_string(),
      lits: Box::new([
        Box::new([TagLit::Pos(tag(1))]),
        Box::new([TagLit::Pos(tag(2)), TagLit::Neg(tag(3))]),
        Box::new([TagLit::Neg(tag(4)), TagLit::Pos(tag(2))]),
      ]),
    };

    let serialized = Json::serialize(&view).unwrap();
    let deserialized = <Json as Backend<View>>::deserialize(&serialized).unwrap();

    assert_eq!(deserialized, view);
  }

  /// Spot-test the conversion of a CNF formula into a "generic" one.
  #[test]
  fn cnf_formula_conversion() {
    let tag1 = tag(1);
    let tag2 = tag(2);
    let tag3 = tag(3);
    let tag4 = tag(4);

    let cnf = Box::from([
      Box::from([TagLit::Pos(tag1)]),
      Box::from([TagLit::Pos(tag2), TagLit::Neg(tag3)]),
      Box::from([TagLit::Neg(tag4), TagLit::Pos(tag2)]),
    ]);

    let expected = Formula::from_str("1 & (2 | !3) & (!4 | 2)").unwrap();
    assert_eq!(cnf_to_formula(&cnf).unwrap(), expected);
  }

  /// Check that we can convert a formula into an equivalent CNF form.
  #[test]
  fn formula_cnf_conversion() {
    // Formula already in CNF, just not the right type.
    let formula = (var(1) | !var(2) | !var(3)) & (!var(4) | var(5));
    let expected = Box::from([
      Box::from([
        TagLit::Pos(tag(1)),
        TagLit::Neg(tag(2)),
        TagLit::Neg(tag(3)),
      ]),
      Box::from([TagLit::Neg(tag(4)), TagLit::Pos(tag(5))]),
    ]);
    let cnf = formula_to_cnf(formula).unwrap();
    assert_eq!(cnf, expected);
  }

  /// Check that the `Formula` object produced by parsing a string and
  /// by converting a CNF formula into a regular `Formula` are
  /// equivalent.
  #[test]
  fn parsing_cnf_conversion_equivalence() {
    fn test(input: &str) {
      let formula = Formula::from_str(input).unwrap();
      let cnf = formula_to_cnf(formula.clone()).unwrap();
      let new = cnf_to_formula(&cnf).unwrap();
      assert_eq!(new, formula);
    }

    test("1 & !2 & !3");
    test("1 | !2 | !3");
  }
}
