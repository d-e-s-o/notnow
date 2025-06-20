// Copyright (C) 2018-2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module providing serialization and deserialization support for
//! task views.

use std::fmt::Debug;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::str::FromStr as _;

use serde::Deserialize;
use serde::Serialize;

use crate::formula::Formula;


/// A literal that can be serialized and deserialized.
#[derive(Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TagLit {
  Pos(String),
  Neg(String),
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
  /// Retrieve the name of the tag.
  pub fn name(&self) -> &str {
    match self {
      TagLit::Pos(tag) | TagLit::Neg(tag) => tag,
    }
  }
}

impl From<&TagLit> for Formula {
  fn from(other: &TagLit) -> Self {
    match other {
      TagLit::Pos(tag) => Formula::Var(tag.to_string()),
      TagLit::Neg(tag) => !Formula::Var(tag.to_string()),
    }
  }
}


#[derive(Clone, Debug, Default)]
pub struct FormulaPair {
  /// The textual representation of the formula.
  pub string: String,
  /// The parsed formula.
  pub formula: Option<Formula>,
}

// We sport a custom `PartialEq` impl here, because we treat only the
// string as the source of the truth. Basically, when serializing such
// an object `formula` may not be filled in, but we still need objects
// with equal string representation to be considered equal for our
// warn-on-unsaved-changes logic.
impl PartialEq for FormulaPair {
  fn eq(&self, other: &Self) -> bool {
    self.string == other.string
  }
}

impl Eq for FormulaPair {}

impl From<Formula> for FormulaPair {
  fn from(other: Formula) -> Self {
    Self {
      string: other.to_string(),
      formula: Some(other),
    }
  }
}


mod formula {
  use super::*;

  use serde::de::Error;
  use serde::de::Unexpected;
  use serde::Deserialize;
  use serde::Deserializer;
  use serde::Serializer;


  /// Deserialize a [`Formula`] value.
  pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<FormulaPair, D::Error>
  where
    D: Deserializer<'de>,
  {
    let s = Option::<String>::deserialize(deserializer)?;
    let f = match s.as_deref() {
      Some(s) if !s.is_empty() => {
        // TODO: By using what currently are implementation details it
        //       would be possible to provide specifically the bits that
        //       could not be parsed.
        let f = Formula::from_str(s)
          .map_err(|_err| Error::invalid_value(Unexpected::Str(s), &"a logical formula"))?;
        Some(f)
      },
      _ => None,
    };

    let f = FormulaPair {
      string: s.unwrap_or_default(),
      formula: f,
    };
    Ok(f)
  }

  /// Serialize a [`FormulaPair`] value.
  pub(crate) fn serialize<S>(f: &FormulaPair, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    serializer.serialize_str(&f.string)
  }
}


/// A view that can be serialized and deserialized.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct View {
  pub name: String,
  #[serde(with = "formula")]
  pub formula: FormulaPair,
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
      Formula::Var(a) => Some(vec![vec![TagLit::Pos(a)]]),
      Formula::Not(a) => match *a {
        Formula::Var(b) => Some(vec![vec![TagLit::Neg(b)]]),
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
              .cloned()
              .chain(bx.iter().cloned())
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

  use crate::ser::backends::Backend;
  use crate::ser::backends::Json;


  /// Check that we can serialize and deserialize a `View`.
  #[test]
  fn serialize_deserialize_view() {
    fn test(formula: FormulaPair) {
      let view = View {
        name: "test-view".to_string(),
        formula,
      };

      let serialized = Json::serialize(&view).unwrap();
      let deserialized = <Json as Backend<View>>::deserialize(&serialized).unwrap();

      assert_eq!(deserialized, view);
    }

    let formula = Formula::from_str("a & (b | !c) & (!d | b)").unwrap();
    let () = test(FormulaPair::from(formula));
    let () = test(FormulaPair::default());
  }

  /// Spot-test the conversion of a CNF formula into a "generic" one.
  #[test]
  fn cnf_formula_conversion() {
    let cnf = Box::from([
      Box::from([TagLit::Pos("a".to_string())]),
      Box::from([TagLit::Pos("b".to_string()), TagLit::Neg("c".to_string())]),
      Box::from([TagLit::Neg("d".to_string()), TagLit::Pos("b".to_string())]),
    ]);

    let expected = Formula::from_str("a & (b | !c) & (!d | b)").unwrap();
    assert_eq!(cnf_to_formula(&cnf).unwrap(), expected);
  }

  /// Check that we can convert a formula into an equivalent CNF form.
  #[test]
  fn formula_cnf_conversion() {
    // Formula already in CNF, just not the right type.
    let formula = (Formula::var("a") | !Formula::var("b") | !Formula::var("c"))
      & (!Formula::var("d") | Formula::var("e"));
    let expected = Box::from([
      Box::from([
        TagLit::Pos("a".to_string()),
        TagLit::Neg("b".to_string()),
        TagLit::Neg("c".to_string()),
      ]),
      Box::from([TagLit::Neg("d".to_string()), TagLit::Pos("e".to_string())]),
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
      let new = cnf_to_formula::<TagLit>(&cnf).unwrap();
      assert_eq!(new, formula);
    }

    test("a & !b & !c");
    test("a | !b | !c");
  }
}
