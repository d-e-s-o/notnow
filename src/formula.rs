// Copyright (C) 2025 Daniel Mueller <deso@posteo.net>
// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;
use std::num::ParseIntError;
use std::ops::BitAnd;
use std::ops::BitOr;
use std::ops::Not;
use std::str::FromStr;

use anyhow::anyhow;
use anyhow::Error;


type ParseResult<'i, Output> = Result<(&'i str, Output), &'i str>;
type Var = usize;


/// A trait representing something that can parse data from a string.
trait Parser<'i> {
  type Output: 'i;

  fn parse(&self, input: &'i str) -> ParseResult<'i, Self::Output>;

  /// Combine two parsers, running both in succession.
  fn chain<P, O>(self, parser: P) -> impl Parser<'i, Output = (Self::Output, O)>
  where
    Self: Sized,
    P: Parser<'i, Output = O>,
    O: 'i,
  {
    move |input| match self.parse(input) {
      Ok((next_input, result1)) => match parser.parse(next_input) {
        Ok((final_input, result2)) => Ok((final_input, (result1, result2))),
        Err(_) => Err(input),
      },
      Err(_) => Err(input),
    }
  }

  fn or<P>(self, parser: P) -> impl Parser<'i, Output = Self::Output>
  where
    Self: Sized,
    P: Parser<'i, Output = Self::Output>,
  {
    move |input| {
      self.parse(input).or_else(|err_input| {
        debug_assert_eq!(input, err_input);
        parser.parse(input)
      })
    }
  }

  fn and_then<F, O, P>(self, f: F) -> impl Parser<'i, Output = O>
  where
    Self: Sized,
    P: Parser<'i, Output = O>,
    F: Fn(Self::Output) -> P,
    O: 'i,
  {
    move |input| match self.parse(input) {
      Ok((next_input, result)) => match f(result).parse(next_input) {
        Ok((final_input, result2)) => Ok((final_input, result2)),
        Err(_) => Err(input),
      },
      Err(_) => Err(input),
    }
  }

  /// Create a new parser that applies a mapping function to this
  /// parser's output.
  fn map<F, O>(self, map_fn: F) -> impl Parser<'i, Output = O>
  where
    Self: Sized,
    F: Fn(Self::Output) -> O,
    O: 'i,
  {
    move |input| {
      self
        .parse(input)
        .map(|(next_input, result)| (next_input, map_fn(result)))
    }
  }
}

impl<'i, F, O> Parser<'i> for F
where
  F: Fn(&'i str) -> Result<(&'i str, O), &'i str>,
  O: 'i,
{
  type Output = O;

  fn parse(&self, input: &'i str) -> ParseResult<'i, Self::Output> {
    (self)(input)
  }
}


/// Parse a number from a string.
///
/// # Notes
/// This function assumes that `N` is an *unsigned* integer type.
fn parse_number<'i, N>(input: &'i str) -> ParseResult<'i, N>
where
  N: FromStr<Err = ParseIntError>,
{
  let mut end = 0;

  for c in input.chars() {
    if c.is_ascii_digit() {
      end += c.len_utf8();
    } else {
      break;
    }
  }

  if end == 0 {
    return Err(input)
  }

  let (num, rest) = input.split_at(end);
  let num = N::from_str(num).map_err(|_err| num)?;
  Ok((rest, num))
}

fn parse_var(input: &str) -> ParseResult<'_, Var> {
  parse_number::<Var>(input)
}


/// Create a parser for the given string.
fn match_str<'i>(s: &'static str) -> impl Parser<'i, Output = ()> {
  move |input: &'i str| match input.get(0..s.len()) {
    Some(next) if next == s => Ok((&input[s.len()..], ())),
    _ => Err(input),
  }
}

fn parse_not(input: &str) -> ParseResult<'_, ()> {
  match_str("!").parse(input)
}

fn parse_or(input: &str) -> ParseResult<'_, ()> {
  match_str("|").parse(input)
}

fn parse_and(input: &str) -> ParseResult<'_, ()> {
  match_str("&").parse(input)
}


/// A logical formula.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Formula {
  Var(Var),
  Not(Box<Formula>),
  And(Box<Formula>, Box<Formula>),
  Or(Box<Formula>, Box<Formula>),
}

impl BitAnd for Formula {
  type Output = Formula;

  #[inline]
  fn bitand(self, other: Self) -> Self::Output {
    Self::And(Box::new(self), Box::new(other))
  }
}

impl BitAnd for Box<Formula> {
  type Output = Formula;

  #[inline]
  fn bitand(self, other: Box<Formula>) -> Self::Output {
    Formula::And(self, other)
  }
}

impl BitOr for Formula {
  type Output = Formula;

  #[inline]
  fn bitor(self, other: Self) -> Self::Output {
    Self::Or(Box::new(self), Box::new(other))
  }
}

impl BitOr for Box<Formula> {
  type Output = Formula;

  #[inline]
  fn bitor(self, other: Self) -> Self::Output {
    Formula::Or(self, other)
  }
}

impl Not for Formula {
  type Output = Formula;

  #[inline]
  fn not(self) -> Self::Output {
    Self::Not(Box::new(self))
  }
}

impl Not for Box<Formula> {
  type Output = Formula;

  #[inline]
  fn not(self) -> Self::Output {
    Formula::Not(self)
  }
}

impl FromStr for Formula {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    parse_formula(s).map_err(|rest| anyhow!("failed to parse formula starting at `{rest}`"))
  }
}

impl Display for Formula {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    fn print<const T: char>(formula: &Formula, fmt: &mut Formatter<'_>) -> FmtResult {
      match formula {
        Formula::Var(var) => write!(fmt, "{var}")?,
        Formula::Not(formula) => {
          write!(fmt, "!")?;
          let group = !matches!(formula.as_ref(), Formula::Var(..) | Formula::Not(..));

          if group {
            write!(fmt, "(")?;
          }
          print::<T>(formula, fmt)?;
          if group {
            write!(fmt, ")")?;
          }
        },
        Formula::And(formula1, formula2) => {
          let group = T == 'd';
          if group {
            write!(fmt, "(")?;
          }

          print::<'c'>(formula1, fmt)?;
          write!(fmt, " & ")?;
          print::<'c'>(formula2, fmt)?;

          if group {
            write!(fmt, ")")?;
          }
        },
        Formula::Or(formula1, formula2) => {
          let group = T == 'c';
          if group {
            write!(fmt, "(")?;
          }

          print::<'d'>(formula1, fmt)?;
          write!(fmt, " | ")?;
          print::<'d'>(formula2, fmt)?;

          if group {
            write!(fmt, ")")?;
          }
        },
      }
      Ok(())
    }

    print::<'a'>(self, f)
  }
}


fn zero_or_more<'i, P, A, Acc>(parser: P) -> impl Parser<'i, Output = Acc>
where
  P: Parser<'i, Output = A>,
  Acc: Default + Extend<A> + 'i,
{
  move |mut input| {
    let mut result = Acc::default();

    while let Ok((next_input, next_item)) = parser.parse(input) {
      input = next_input;
      result.extend([next_item]);
    }

    Ok((input, result))
  }
}

fn parse_space0(input: &str) -> ParseResult<'_, ()> {
  // TODO: We could get more fancy by accepting arbitrary white spaces
  //       etc.
  zero_or_more::<_, _, ()>(match_str(" ")).parse(input)
}

fn parse_unary(input: &str) -> ParseResult<'_, Formula> {
  let spaces = parse_space0;
  let var = parse_var.map(Formula::Var);
  let negation = parse_not.chain(parse_unary).map(|((), formula)| !formula);
  let grouping = match_str("(")
    .chain(parse_formula_impl::<'a'>)
    .chain(match_str(")"))
    .map(|(((), formula), ())| formula);

  spaces
    .chain(var.or(negation).or(grouping))
    .chain(spaces)
    .map(|(((), formula), ())| formula)
    .parse(input)
}


enum Either<A, B, C> {
  A(A),
  B(B),
  C(C),
}

impl<'i, A, B, C, O> Parser<'i> for Either<A, B, C>
where
  A: Parser<'i, Output = O>,
  B: Parser<'i, Output = O>,
  C: Parser<'i, Output = O>,
  O: 'i,
{
  type Output = O;

  fn parse(&self, input: &'i str) -> ParseResult<'i, Self::Output> {
    match self {
      Self::A(a) => a.parse(input),
      Self::B(b) => b.parse(input),
      Self::C(c) => c.parse(input),
    }
  }
}


/// # Notes
/// The const generic argument determines what subsequent formulas are
/// allowed:
/// - 'c': conjunctions but no disjunctions
/// - 'd': disjunctions but no conjunctions
/// - anything else: no restriction
fn parse_formula_impl<const T: char>(input: &str) -> ParseResult<'_, Formula> {
  parse_unary
    .and_then(|formula1| {
      // Attempt to parse the remainder of the input as a conjunction or
      // disjunction, but just fall back to the formula we already
      // parsed successfully if that fails.
      // TODO: All the clones here are not great, to say the least.
      //       But it's not clear how we can get rid of them either.
      let f11 = formula1.clone();
      let f12 = formula1.clone();
      let conjunction = parse_and
        .chain(parse_formula_impl::<'c'>)
        .map(move |((), formula2)| f11.clone() & formula2);
      let disjunction = parse_or
        .chain(parse_formula_impl::<'d'>)
        .map(move |((), formula2)| f12.clone() | formula2);

      match T {
        'c' => Either::A(conjunction),
        'd' => Either::B(disjunction),
        _ => Either::C(conjunction.or(disjunction)),
      }
      .or(move |input| Ok((input, formula1.clone())))
    })
    .parse(input)
}

/// Parse a formula.
///
/// This is not a "regular" parser function in that it does not
/// implement the [`Parser`] trait. That's because it does not return
/// any unparsed input for subsequent parsers to consume, but ensures
/// that everything has been parsed fully or, if not, report the
/// unparsed remainder as an `Err`.
fn parse_formula(input: &str) -> Result<Formula, &str> {
  let (rest, formula) = parse_formula_impl::<'a'>(input)?;
  if !rest.is_empty() {
    Err(rest)
  } else {
    Ok(formula)
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  #[cfg(feature = "nightly")]
  use std::hint::black_box;

  #[cfg(feature = "nightly")]
  use unstable_test::Bencher;


  /// Check that we can parse a single number.
  #[test]
  fn number_parsing() {
    let (rest, num) = parse_number::<u64>("0").unwrap();
    assert_eq!(rest, "");
    assert_eq!(num, 0);

    let (rest, num) = parse_number::<u64>("1").unwrap();
    assert_eq!(rest, "");
    assert_eq!(num, 1);

    // We currently accept leading zeroes, as it's easier to do so than
    // to explicitly exclude them.
    let (rest, num) = parse_number::<u64>("01").unwrap();
    assert_eq!(rest, "");
    assert_eq!(num, 1);

    let (rest, num) = parse_number::<u64>("12345").unwrap();
    assert_eq!(rest, "");
    assert_eq!(num, 12345);

    let err = parse_number::<u64>("").unwrap_err();
    assert_eq!(err, "");
  }

  /// Test that we can parse a NOT ("!").
  #[test]
  fn not_parsing() {
    let err = parse_not("f").unwrap_err();
    assert_eq!(err, "f");

    let (rest, ()) = parse_not("!").unwrap();
    assert_eq!(rest, "");
  }

  /// Test that we can parse an OR ("||").
  #[test]
  fn or_parsing() {
    let err = parse_or("foobar").unwrap_err();
    assert_eq!(err, "foobar");

    let err = parse_or("x").unwrap_err();
    assert_eq!(err, "x");

    let (rest, ()) = parse_or("|").unwrap();
    assert_eq!(rest, "");
  }

  /// Test that we can parse an AND ("&&").
  #[test]
  fn and_parsing() {
    let err = parse_and("foobar").unwrap_err();
    assert_eq!(err, "foobar");

    let err = parse_and("x").unwrap_err();
    assert_eq!(err, "x");

    let (rest, ()) = parse_and("&").unwrap();
    assert_eq!(rest, "");
  }

  /// Make sure that we can chain parsers as expected.
  #[test]
  fn parser_chaining() {
    let parser = parse_or.chain(parse_and);
    let (rest, ((), ())) = parser.parse("|&").unwrap();
    assert_eq!(rest, "");

    let (rest, ((), ())) = parser.parse("|&x").unwrap();
    assert_eq!(rest, "x");

    let err = parser.parse("|x").unwrap_err();
    assert_eq!(err, "|x");

    let err = parser.parse("x").unwrap_err();
    assert_eq!(err, "x");
  }

  /// Make sure that we can chain parsers as expected.
  #[test]
  fn parser_anding() {
    let parser = parse_var.and_then(|_var| parse_or);
    let (rest, ()) = parser.parse("123|").unwrap();
    assert_eq!(rest, "");

    let err = parser.parse("123").unwrap_err();
    assert_eq!(err, "123");
  }

  /// Check that we can map the output of parsers.
  #[test]
  fn output_mapping() {
    let input = "1234&";
    let (rest, var) = parse_var
      .chain(parse_and)
      .map(|(l, ())| l)
      .parse(input)
      .unwrap();
    assert_eq!(var, 1234);
    assert_eq!(rest, "");

    let input = "|987";
    let (rest, var) = parse_or
      .chain(parse_var)
      .map(|((), r)| r)
      .parse(input)
      .unwrap();
    assert_eq!(var, 987);
    assert_eq!(rest, "");
  }

  /// Make sure that "OR"ing of parsers works as it should.
  #[test]
  fn parser_oring() {
    let input = "|";
    let (rest, ()) = parse_or.or(parse_and).parse(input).unwrap();
    assert_eq!(rest, "");

    let input = "&";
    let (rest, ()) = parse_or.or(parse_and).parse(input).unwrap();
    assert_eq!(rest, "");

    let input = "x";
    let err = parse_or.or(parse_and).parse(input).unwrap_err();
    assert_eq!(err, "x");
  }

  /// Make sure that necessary precedence constraints are adhered to
  /// when parsing.
  #[test]
  fn formula_parsing_precedence() {
    // "NOT" (`!`) should have precedence over "AND".
    let formula = parse_formula("!45&9").unwrap();
    let expected = (!Formula::Var(45)) & Formula::Var(9);
    assert_eq!(formula, expected);

    // To avoid confusion when it comes to precedence between "OR" and
    // "AND", we don't allow intermixing at all and require explicit
    // grouping instead.
    let err = parse_formula("45|9&65").unwrap_err();
    assert_eq!(err, "&65");

    let err = parse_formula("45&9|65").unwrap_err();
    assert_eq!(err, "|65");

    let err = parse_formula("45&9|!65").unwrap_err();
    assert_eq!(err, "|!65");

    let err = parse_formula("45&!9|65").unwrap_err();
    assert_eq!(err, "|65");

    let err = parse_formula("!45&9|65").unwrap_err();
    assert_eq!(err, "|65");
  }

  /// Check that various formulas can be parsed successfully.
  #[test]
  fn formula_parsing() {
    let formula = parse_formula("23").unwrap();
    let expected = Formula::Var(23);
    assert_eq!(formula, expected);

    let formula = parse_formula("!12").unwrap();
    let expected = !Formula::Var(12);
    assert_eq!(formula, expected);

    let formula = parse_formula("!!45").unwrap();
    let expected = !!Formula::Var(45);
    assert_eq!(formula, expected);

    let formula = parse_formula("1&2").unwrap();
    let expected = Formula::Var(1) & Formula::Var(2);
    assert_eq!(formula, expected);

    let formula = parse_formula("1|2").unwrap();
    let expected = Formula::Var(1) | Formula::Var(2);
    assert_eq!(formula, expected);

    let formula = parse_formula("(1|2)&3").unwrap();
    let expected = (Formula::Var(1) | Formula::Var(2)) & Formula::Var(3);
    assert_eq!(formula, expected);

    let formula = parse_formula("!1&(2|3)").unwrap();
    let expected = !Formula::Var(1) & (Formula::Var(2) | Formula::Var(3));
    assert_eq!(formula, expected);

    // Double negation of grouping.
    let formula = parse_formula("!!(45)").unwrap();
    let expected = !!Formula::Var(45);
    assert_eq!(formula, expected);

    let formula = parse_formula("!(!45&9)&8").unwrap();
    let expected = !(!Formula::Var(45) & Formula::Var(9)) & Formula::Var(8);
    assert_eq!(formula, expected);

    let err = parse_formula("xyz").unwrap_err();
    assert_eq!(err, "xyz");
  }

  /// Check that various combinations of white spaces in formulas don't
  /// trip the parser over.
  // TODO: All these tests may be more appropriately captured with a
  //       property based testing scheme.
  #[test]
  fn formula_parsing_whitespaces() {
    // White spaces around literal.
    let formula = parse_formula(" 23").unwrap();
    let expected = Formula::Var(23);
    assert_eq!(formula, expected);

    let formula = parse_formula(" 23   ").unwrap();
    let expected = Formula::Var(23);
    assert_eq!(formula, expected);

    let formula = parse_formula("23 ").unwrap();
    let expected = Formula::Var(23);
    assert_eq!(formula, expected);

    let formula = parse_formula("!  12").unwrap();
    let expected = !Formula::Var(12);
    assert_eq!(formula, expected);

    let formula = parse_formula("  !  12 ").unwrap();
    let expected = !Formula::Var(12);
    assert_eq!(formula, expected);

    let formula = parse_formula("  !   !  12 ").unwrap();
    let expected = !!Formula::Var(12);
    assert_eq!(formula, expected);

    let formula = parse_formula("1  & 2  ").unwrap();
    let expected = Formula::Var(1) & Formula::Var(2);
    assert_eq!(formula, expected);

    let formula = parse_formula("(  45 )").unwrap();
    let expected = Formula::Var(45);
    assert_eq!(formula, expected);

    let formula = parse_formula("!  ! (  45 )").unwrap();
    let expected = !!Formula::Var(45);
    assert_eq!(formula, expected);

    let formula = parse_formula("!  (  !45&    9) &8   ").unwrap();
    let expected = !(!Formula::Var(45) & Formula::Var(9)) & Formula::Var(8);
    assert_eq!(formula, expected);
  }

  /// Check that various invalid formulas cause parsing errors.
  #[test]
  fn formula_parsing_error() {
    // Trailing negation symbol.
    let err = parse_formula("!!45!").unwrap_err();
    assert_eq!(err, "!");

    let err = parse_formula("(1&2").unwrap_err();
    assert_eq!(err, "(1&2");

    let err = parse_formula("1&&2").unwrap_err();
    assert_eq!(err, "&&2");
  }

  /// Make sure that our formula formatting works as expected.
  #[test]
  fn formula_displaying() {
    fn test(input: &str) {
      let formula = parse_formula(input).unwrap();
      let s = formula.to_string();
      assert_eq!(s, input);
      assert_eq!(parse_formula(&s).unwrap(), formula);
    }

    test("42");
    test("1 | 2");
    test("(1 & 2) | 3");
    test("(1 & 2 & 3) | 4");
    test("1 | (2 & 3)");
    test("1 & !2 & !3");
    test("!(!45 & 9) & 8");
    test("!!(1 | 2)");
  }

  /// Benchmark the parsing of a formula.
  #[cfg(feature = "nightly")]
  #[bench]
  fn bench_formula_parsing(b: &mut Bencher) {
    let formula = "(!1&!2)|((43&!!!6)|(2&3))";
    let () = b.iter(|| {
      let _formula = black_box(parse_formula(black_box(formula)).unwrap());
    });
  }
}
