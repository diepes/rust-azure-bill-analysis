use serde::Deserialize;
use std::fmt;
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

/// Amount in US dollars (the pricing currency Azure uses for unit/effective prices).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Deserialize)]
pub struct Usd(pub f64);

/// Amount in New Zealand dollars (the billing currency for this account).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Deserialize)]
pub struct Nzd(pub f64);

fn format_amount(value: f64, decimal_places: usize) -> String {
    let formatted = format!("{:.*}", decimal_places, value.abs());
    let parts: Vec<&str> = formatted.split('.').collect();
    let integer_part = parts[0];
    let decimal_part = if parts.len() > 1 { parts[1] } else { "" };
    let mut formatted_integer = String::new();
    for (i, c) in integer_part.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            formatted_integer.push(',');
        }
        formatted_integer.push(c);
    }
    let formatted_integer: String = formatted_integer.chars().rev().collect();
    let padded_decimal = format!("{:0<width$}", decimal_part, width = decimal_places);
    let sign = if value < 0.0 { "-" } else { " " };
    if decimal_places > 0 {
        format!("{}{}.{}", sign, formatted_integer, padded_decimal)
    } else {
        format!("{}{}", sign, formatted_integer)
    }
}

impl fmt::Display for Usd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "US${}", format_amount(self.0, 2))
    }
}

impl fmt::Display for Nzd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NZ${}", format_amount(self.0, 2))
    }
}

impl Usd {
    pub fn to_nzd(self, rate: f64) -> Nzd {
        Nzd(self.0 * rate)
    }
    pub fn amount(self) -> f64 {
        self.0
    }
}

impl Nzd {
    pub fn to_usd(self, rate: f64) -> Usd {
        Usd(self.0 / rate)
    }
    pub fn amount(self) -> f64 {
        self.0
    }
}

macro_rules! impl_money_ops {
    ($T:ident) => {
        impl Add for $T {
            type Output = $T;
            fn add(self, rhs: $T) -> $T { $T(self.0 + rhs.0) }
        }
        impl AddAssign for $T {
            fn add_assign(&mut self, rhs: $T) { self.0 += rhs.0; }
        }
        impl Sub for $T {
            type Output = $T;
            fn sub(self, rhs: $T) -> $T { $T(self.0 - rhs.0) }
        }
        impl SubAssign for $T {
            fn sub_assign(&mut self, rhs: $T) { self.0 -= rhs.0; }
        }
        impl Neg for $T {
            type Output = $T;
            fn neg(self) -> $T { $T(-self.0) }
        }
        impl Mul<f64> for $T {
            type Output = $T;
            fn mul(self, rhs: f64) -> $T { $T(self.0 * rhs) }
        }
        impl std::iter::Sum for $T {
            fn sum<I: Iterator<Item = $T>>(iter: I) -> $T {
                iter.fold($T(0.0), |a, b| a + b)
            }
        }
    };
}

impl_money_ops!(Usd);
impl_money_ops!(Nzd);
