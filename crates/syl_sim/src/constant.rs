//! Evaluate the two static spellings used in HWIR (arithmetic dimensions and
//! functional EIR fact keys for parameter bindings). No text reaches an executor.
use crate::{ParamValue, SimError};
use std::collections::BTreeMap;
use syl_hw::{HwBinaryOp as Op, HwExpr, HwUnaryOp};

pub(crate) type Bindings = BTreeMap<String, ParamValue>;
pub(crate) type Result<T> = std::result::Result<T, SimError>;

pub(crate) fn invalid<T>(message: impl Into<String>) -> Result<T> {
    Err(SimError::Invalid(message.into()))
}

impl ParamValue {
    pub(crate) fn number(self) -> u64 {
        match self {
            Self::Nat(n) => n,
            Self::Bool(b) => u64::from(b),
        }
    }
}

fn binary(op: Op, a: u64, b: u64) -> Result<u64> {
    let n = match op {
        Op::Add => a.checked_add(b),
        Op::Sub => a.checked_sub(b),
        Op::Mul => a.checked_mul(b),
        Op::Div => a.checked_div(b),
        Op::Rem => a.checked_rem(b),
        Op::Shl => u32::try_from(b)
            .ok()
            .filter(|s| *s < 64 && a <= (u64::MAX >> s))
            .map(|s| a << s),
        Op::Eq => Some((a == b).into()),
        Op::NotEq => Some((a != b).into()),
        Op::Lt => Some((a < b).into()),
        Op::LtEq => Some((a <= b).into()),
        Op::Gt => Some((a > b).into()),
        Op::GtEq => Some((a >= b).into()),
        Op::BitAnd => Some(a & b),
        Op::BitOr => Some(a | b),
        Op::BitXor => Some(a ^ b),
        Op::OrOr => Some((a != 0 || b != 0).into()),
        Op::AndAnd => Some((a != 0 && b != 0).into()),
        _ => return invalid(format!("unsupported static operation {op:?}")),
    };
    n.ok_or_else(|| SimError::Invalid(format!("invalid nat operation {a} {op:?} {b}")))
}

pub(crate) fn eval(expr: &HwExpr, env: &Bindings) -> Result<u64> {
    match expr {
        HwExpr::Ident(name) => lookup(name, env),
        HwExpr::Int(n) => Ok(*n),
        HwExpr::Bool(b) => Ok((*b).into()),
        HwExpr::Zero => Ok(0),
        HwExpr::Unary {
            op: HwUnaryOp::Not,
            expr,
        } => Ok((eval(expr, env)? == 0).into()),
        HwExpr::Unary {
            op: HwUnaryOp::Neg,
            expr,
        } => binary(Op::Sub, 0, eval(expr, env)?),
        HwExpr::Binary { op, left, right } => binary(*op, eval(left, env)?, eval(right, env)?),
        HwExpr::Mux {
            cond,
            then_value,
            else_value,
        } => eval(
            if eval(cond, env)? != 0 {
                then_value
            } else {
                else_value
            },
            env,
        ),
        HwExpr::Select { arms, default, .. } => {
            for arm in arms {
                if eval(arm.guard(), env)? != 0 {
                    return eval(arm.value(), env);
                }
            }
            eval(default, env)
        }
        HwExpr::Call { name, args } if name == "clog2" && args.len() == 1 => {
            let value = eval(&args[0], env)?;
            Ok(if value <= 1 {
                0
            } else {
                (u64::BITS - (value - 1).leading_zeros()) as u64
            })
        }
        _ => invalid(format!("expected a static expression, got {expr:?}")),
    }
}

fn lookup(name: &str, env: &Bindings) -> Result<u64> {
    match name {
        "true" => Ok(1),
        "false" => Ok(0),
        _ => env
            .get(name)
            .copied()
            .map(ParamValue::number)
            .ok_or_else(|| SimError::Invalid(format!("unbound static name {name}"))),
    }
}

pub(crate) fn dimension(source: &str, env: &Bindings) -> Result<u32> {
    let value = number(source, env)?;
    u32::try_from(value).ok().filter(|n| *n > 0).ok_or_else(|| {
        SimError::Invalid(format!("dimension {source} = {value} outside 1..=u32::MAX"))
    })
}

pub(crate) fn number(source: &str, env: &Bindings) -> Result<u64> {
    let mut parser = Parser { rest: source, env };
    let value = parser.infix(0)?;
    if !parser.rest.trim().is_empty() {
        return invalid(format!("invalid static expression {source}"));
    }
    Ok(value)
}

struct Parser<'a> {
    rest: &'a str,
    env: &'a Bindings,
}
impl Parser<'_> {
    fn eat(&mut self, token: &str) -> bool {
        if let Some(rest) = self.rest.trim_start().strip_prefix(token) {
            self.rest = rest;
            true
        } else {
            false
        }
    }

    fn infix(&mut self, precedence: u8) -> Result<u64> {
        const OPS: &[(&str, u8, Op)] = &[
            ("||", 1, Op::OrOr),
            ("&&", 2, Op::AndAnd),
            ("or", 3, Op::BitOr),
            ("xor", 4, Op::BitXor),
            ("and", 5, Op::BitAnd),
            ("==", 6, Op::Eq),
            ("!=", 6, Op::NotEq),
            ("<<", 8, Op::Shl),
            ("<=", 7, Op::LtEq),
            (">=", 7, Op::GtEq),
            ("<", 7, Op::Lt),
            (">", 7, Op::Gt),
            ("+", 9, Op::Add),
            ("-", 9, Op::Sub),
            ("*", 10, Op::Mul),
            ("/", 10, Op::Div),
            ("%", 10, Op::Rem),
        ];
        let mut value = self.atom()?;
        while let Some(&(token, level, op)) = OPS
            .iter()
            .find(|(token, _, _)| self.rest.trim_start().starts_with(token))
        {
            if level < precedence {
                break;
            }
            self.eat(token);
            value = binary(op, value, self.infix(level + 1)?)?;
        }
        Ok(value)
    }

    fn atom(&mut self) -> Result<u64> {
        if self.eat("!") {
            return Ok((self.atom()? == 0).into());
        }
        if self.eat("-") {
            return binary(Op::Sub, 0, self.atom()?);
        }
        if self.eat("(") {
            let value = self.infix(0)?;
            self.expect(")")?;
            return Ok(value);
        }
        let rest = self.rest.trim_start();
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if end == 0 {
            return invalid(format!("expected static operand at {rest}"));
        }
        let key = &rest[..end];
        self.rest = &rest[end..];
        if !self.eat("(") {
            if key.as_bytes()[0].is_ascii_digit() {
                return key
                    .parse()
                    .map_err(|_| SimError::Invalid(format!("invalid nat {key}")));
            }
            return lookup(key, self.env);
        }
        let a = self.infix(0)?;
        if matches!(key, "not" | "neg") {
            self.expect(")")?;
            return if key == "not" {
                Ok((a == 0).into())
            } else {
                binary(Op::Sub, 0, a)
            };
        }
        self.expect(",")?;
        let b = self.infix(0)?;
        if key == "mux" {
            self.expect(",")?;
            let c = self.infix(0)?;
            self.expect(")")?;
            return Ok(if a != 0 { b } else { c });
        }
        self.expect(")")?;
        let op = match key {
            "add" => Op::Add,
            "sub" => Op::Sub,
            "mul" => Op::Mul,
            "div" => Op::Div,
            "rem" => Op::Rem,
            "shl" => Op::Shl,
            "eq" => Op::Eq,
            "not_eq" => Op::NotEq,
            "lt" => Op::Lt,
            "lt_eq" => Op::LtEq,
            "gt" => Op::Gt,
            "gt_eq" => Op::GtEq,
            "bit_and" => Op::BitAnd,
            "bit_or" => Op::BitOr,
            "bit_xor" => Op::BitXor,
            "logic_and" => Op::AndAnd,
            "logic_or" => Op::OrOr,
            _ => return invalid(format!("unknown static function {key}")),
        };
        binary(op, a, b)
    }

    fn expect(&mut self, token: &str) -> Result<()> {
        if self.eat(token) {
            Ok(())
        } else {
            invalid(format!("expected {token} at {}", self.rest))
        }
    }
}
