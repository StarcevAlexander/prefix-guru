use std::fmt;
use std::sync::OnceLock;

use postcss_guru::{NodeId, NodeKind};
use regex::Regex;

use crate::engine::Session;
use crate::utils::{self, uniq};

#[derive(Clone)]
pub(crate) struct AtRule {
    pub prefixes: Vec<String>,
}

impl AtRule {
    pub(crate) fn process(&self, ctx: &mut Session, node: NodeId) {
        let parent = ctx.parent_prefix(node);
        for prefix in &self.prefixes {
            if parent.as_ref().is_none_or(|parent| parent == prefix) {
                self.add(ctx, node, prefix);
            }
        }
    }

    fn add(&self, ctx: &mut Session, rule: NodeId, prefix: &str) {
        let prefixed = format!("{prefix}{}", ctx.tree.name(rule));
        let params = ctx.tree.params(rule).to_string();
        let already = ctx.parent_has(rule, |tree, child| {
            tree.is(child, NodeKind::AtRule)
                && tree.name(child) == prefixed
                && tree.params(child) == params
        });
        if already {
            return;
        }
        let cloned = ctx.clone(rule);
        ctx.tree.set_name(cloned, prefixed);
        ctx.tree.insert_before(rule, cloned);
    }

    pub(crate) fn clean_resolution(&self, ctx: &mut Session, rule: NodeId) {
        let params = ctx.tree.params(rule).to_string();
        let bad: Vec<String> = self
            .prefixes
            .iter()
            .flat_map(|prefix| [prefix_name(prefix, "min"), prefix_name(prefix, "max")])
            .collect();
        let cleaned = utils::edit_list(&params, |queries, _| {
            Some(
                queries
                    .into_iter()
                    .filter(|query| bad.iter().all(|bad| !query.contains(bad.as_str())))
                    .collect(),
            )
        });
        ctx.tree.set_params(rule, cleaned);
    }

    pub(crate) fn process_resolution(&self, ctx: &mut Session, rule: NodeId) {
        let parent = ctx.parent_prefix(rule);
        let prefixes: Vec<String> =
            parent.map_or_else(|| self.prefixes.clone(), |parent| vec![parent]);
        static REGEXP: OnceLock<Regex> = OnceLock::new();
        static SPLIT: OnceLock<Regex> = OnceLock::new();
        let regexp = REGEXP.get_or_init(|| {
            Regex::new(r"(?i)(min|max)-resolution\s*:\s*\d*\.?\d+(dppx|dpcm|dpi|x)").unwrap()
        });
        let split = SPLIT.get_or_init(|| {
            Regex::new(r"(?i)(min|max)-resolution(\s*:\s*)(\d*\.?\d+)(dppx|dpcm|dpi|x)").unwrap()
        });
        let params = ctx.tree.params(rule).to_string();
        let processed = utils::edit_list(&params, |origin, prefixed| {
            for query in origin {
                if !query.contains("min-resolution") && !query.contains("max-resolution") {
                    prefixed.push(query.clone());
                    continue;
                }
                for prefix in &prefixes {
                    let converted = regexp
                        .replace_all(&query, |captures: &regex::Captures| {
                            let parts = split.captures(&captures[0]).expect("split matches");
                            prefix_query(prefix, &parts[1], &parts[2], &parts[3], &parts[4])
                        })
                        .into_owned();
                    prefixed.push(converted);
                }
                prefixed.push(query);
            }
            Some(uniq(prefixed))
        });
        ctx.tree.set_params(rule, processed);
    }
}

fn prefix_name(prefix: &str, name: &str) -> String {
    if prefix == "-moz-" {
        format!("{name}--moz-device-pixel-ratio")
    } else {
        format!("{prefix}{name}-device-pixel-ratio")
    }
}

fn prefix_query(prefix: &str, name: &str, colon: &str, value: &str, units: &str) -> String {
    let mut value = Fraction::parse(value);
    if units == "dpi" {
        value = value.div(&Fraction::from_i128(96));
    } else if units == "dpcm" {
        value = value
            .mul(&Fraction::parse("2.54"))
            .div(&Fraction::from_i128(96));
    }
    value = value.simplify();
    let text = if prefix == "-o-" {
        format!("{}/{}", value.n, value.d)
    } else {
        value.to_string()
    };
    format!("{}{colon}{text}", prefix_name(prefix, name))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fraction {
    sign: i128,
    n: i128,
    d: i128,
}

fn gcd(a: i128, b: i128) -> i128 {
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }
    let mut a = a;
    let mut b = b;
    loop {
        a %= b;
        if a == 0 {
            return b;
        }
        b %= a;
        if b == 0 {
            return a;
        }
    }
}

fn new_fraction(n: i128, d: i128) -> Fraction {
    assert!(d != 0, "Division by zero");
    let sign = if n < 0 { -1 } else { 1 };
    let n = n.abs();
    let a = gcd(n, d);
    Fraction {
        sign,
        n: n / a,
        d: d / a,
    }
}

impl Fraction {
    fn from_i128(value: i128) -> Self {
        new_fraction(value, 1)
    }

    fn parse(text: &str) -> Self {
        let letters: Vec<char> = text.chars().collect();
        let mut at = 0;
        let mut sign = 1i128;
        if letters.first() == Some(&'-') {
            sign = -1;
            at += 1;
        } else if letters.first() == Some(&'+') {
            at += 1;
        }
        let digits = |from: usize| -> Option<(usize, i128)> {
            let start = from;
            let mut end = from;
            while end < letters.len() && letters[end].is_ascii_digit() {
                end += 1;
            }
            (end > start).then(|| {
                let value: i128 = letters[start..end]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0);
                (end, value)
            })
        };
        let Some((mut at, value)) = digits(at) else {
            return new_fraction(0, 1);
        };
        if at == letters.len() {
            return new_fraction(sign * value, 1);
        }
        let Some('.') = letters.get(at).copied() else {
            return new_fraction(sign * value, 1);
        };
        at += 1;
        let (fraction, digits_count) = match digits(at) {
            Some((end, fraction)) => (fraction, end - at),
            None => (0, 0),
        };
        let (n, d) = (
            value * 10i128.pow(digits_count as u32) + fraction,
            10i128.pow(digits_count as u32),
        );
        new_fraction(sign * n, d)
    }

    fn mul(&self, other: &Self) -> Self {
        new_fraction(self.sign * other.sign * self.n * other.n, self.d * other.d)
    }

    fn div(&self, other: &Self) -> Self {
        new_fraction(self.sign * other.sign * self.n * other.d, self.d * other.n)
    }

    fn add(&self, other: &Self) -> Self {
        new_fraction(
            self.sign * self.n * other.d + other.sign * other.n * self.d,
            self.d * other.d,
        )
    }

    fn inverse(&self) -> Self {
        new_fraction(self.sign * self.d, self.n)
    }

    fn simplify(&self) -> Self {
        let mut a = self.n;
        let mut b = self.d;
        let mut continued = Vec::new();
        while b != 0 {
            continued.push(a / b);
            let rest = a % b;
            a = b;
            b = rest;
        }
        for i in 1..continued.len() {
            let mut s = new_fraction(continued[i - 1], 1);
            for k in (0..i - 1).rev() {
                s = s.inverse().add(&new_fraction(continued[k], 1));
            }
            let t = new_fraction(s.n * self.d - self.n * s.d, s.d * self.d);
            if t.n * 1000 < t.d {
                return new_fraction(self.sign * s.n, s.d);
            }
        }
        *self
    }

    fn render(&self) -> String {
        let (mut n, mut d) = (self.n, self.d);
        let mut cycle = 0i128;
        while d % 2 == 0 {
            d /= 2;
        }
        while d % 5 == 0 {
            d /= 5;
        }
        if d != 1 {
            let mut rest = 10 % d;
            let mut length = 1;
            while rest != 1 && length <= 2000 {
                rest = rest * 10 % d;
                length += 1;
            }
            if rest == 1 {
                cycle = length;
            }
        }
        d = self.d;
        let mut offset = 0;
        if cycle > 0 {
            let mut rem1 = 1;
            let mut rem2 = modpow(10, cycle as u32, d);
            for t in 0..300 {
                if rem1 == rem2 {
                    offset = t;
                    break;
                }
                rem1 = rem1 * 10 % d;
                rem2 = rem2 * 10 % d;
            }
        }
        let mut out = if self.sign < 0 {
            String::from("-")
        } else {
            String::new()
        };
        out.push_str(&(n / d).to_string());
        n %= d;
        n *= 10;
        if n != 0 {
            out.push('.');
        }
        if cycle > 0 {
            for _ in 0..offset {
                out.push_str(&(n / d).to_string());
                n %= d;
                n *= 10;
            }
            out.push('(');
            for _ in 0..cycle {
                out.push_str(&(n / d).to_string());
                n %= d;
                n *= 10;
            }
            out.push(')');
        } else {
            let mut digits = 15;
            while n != 0 && digits > 0 {
                out.push_str(&(n / d).to_string());
                n %= d;
                n *= 10;
                digits -= 1;
            }
        }
        out
    }
}

impl fmt::Display for Fraction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.render())
    }
}

fn modpow(base: i128, exponent: u32, modulus: i128) -> i128 {
    let mut result = 1;
    let mut base = base % modulus;
    let mut exponent = exponent;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result = result * base % modulus;
        }
        base = base * base % modulus;
        exponent >>= 1;
    }
    result
}
