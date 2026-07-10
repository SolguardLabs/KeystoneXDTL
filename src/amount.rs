use core::fmt;
use core::ops::{Add, AddAssign, Sub, SubAssign};

use serde::{Deserialize, Serialize};

use crate::{KeystoneError, KeystoneResult};

pub const BPS_DENOMINATOR: u128 = 10_000;
pub const DECIMAL_SCALE: u128 = 1_000_000_000_000;

#[derive(
    Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Amount(pub u64);

impl Amount {
    pub const ZERO: Amount = Amount(0);
    pub const ONE: Amount = Amount(1);

    pub fn new(value: u64) -> KeystoneResult<Self> {
        Ok(Self(value))
    }

    pub fn non_zero(value: u64) -> KeystoneResult<Self> {
        if value == 0 {
            return Err(KeystoneError::ZeroAmount);
        }
        Ok(Self(value))
    }

    pub fn raw(self) -> u64 {
        self.0
    }

    pub fn as_u128(self) -> u128 {
        self.0 as u128
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn checked_add(self, rhs: Amount) -> KeystoneResult<Amount> {
        self.0
            .checked_add(rhs.0)
            .map(Amount)
            .ok_or(KeystoneError::AmountOverflow)
    }

    pub fn checked_sub(self, rhs: Amount) -> KeystoneResult<Amount> {
        self.0
            .checked_sub(rhs.0)
            .map(Amount)
            .ok_or(KeystoneError::AmountUnderflow)
    }

    pub fn checked_mul_u64(self, rhs: u64) -> KeystoneResult<Amount> {
        let value = self
            .as_u128()
            .checked_mul(rhs as u128)
            .ok_or(KeystoneError::AmountOverflow)?;
        Amount::from_u128(value)
    }

    pub fn checked_div_u64(self, rhs: u64) -> KeystoneResult<Amount> {
        if rhs == 0 {
            return Err(KeystoneError::DivisionByZero);
        }
        Ok(Amount(self.0 / rhs))
    }

    pub fn saturating_sub(self, rhs: Amount) -> Amount {
        Amount(self.0.saturating_sub(rhs.0))
    }

    pub fn min(self, rhs: Amount) -> Amount {
        if self <= rhs { self } else { rhs }
    }

    pub fn max(self, rhs: Amount) -> Amount {
        if self >= rhs { self } else { rhs }
    }

    pub fn ceil_div(self, rhs: u64) -> KeystoneResult<Amount> {
        if rhs == 0 {
            return Err(KeystoneError::DivisionByZero);
        }
        if self.0 == 0 {
            return Ok(Amount::ZERO);
        }
        Ok(Amount(1 + ((self.0 - 1) / rhs)))
    }

    pub fn from_u128(value: u128) -> KeystoneResult<Amount> {
        if value > u64::MAX as u128 {
            return Err(KeystoneError::AmountOverflow);
        }
        Ok(Amount(value as u64))
    }

    pub fn proportion_floor(self, numerator: u128, denominator: u128) -> KeystoneResult<Amount> {
        if denominator == 0 {
            return Err(KeystoneError::DivisionByZero);
        }
        Amount::from_u128(self.as_u128() * numerator / denominator)
    }

    pub fn proportion_ceil(self, numerator: u128, denominator: u128) -> KeystoneResult<Amount> {
        if denominator == 0 {
            return Err(KeystoneError::DivisionByZero);
        }
        let product = self
            .as_u128()
            .checked_mul(numerator)
            .ok_or(KeystoneError::AmountOverflow)?;
        Amount::from_u128(product.div_ceil(denominator))
    }

    pub fn checked_sum(values: impl IntoIterator<Item = Amount>) -> KeystoneResult<Amount> {
        let mut total = Amount::ZERO;
        for value in values {
            total = total.checked_add(value)?;
        }
        Ok(total)
    }
}

impl Add for Amount {
    type Output = Amount;

    fn add(self, rhs: Amount) -> Self::Output {
        self.checked_add(rhs)
            .expect("amount addition overflow in operator")
    }
}

impl Sub for Amount {
    type Output = Amount;

    fn sub(self, rhs: Amount) -> Self::Output {
        self.checked_sub(rhs)
            .expect("amount subtraction underflow in operator")
    }
}

impl AddAssign for Amount {
    fn add_assign(&mut self, rhs: Amount) {
        *self = *self + rhs;
    }
}

impl SubAssign for Amount {
    fn sub_assign(&mut self, rhs: Amount) {
        *self = *self - rhs;
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(
    Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Shares(pub u64);

impl Shares {
    pub const ZERO: Shares = Shares(0);
    pub const ONE: Shares = Shares(1);

    pub fn new(value: u64) -> KeystoneResult<Self> {
        Ok(Self(value))
    }

    pub fn non_zero(value: u64) -> KeystoneResult<Self> {
        if value == 0 {
            return Err(KeystoneError::ZeroShares);
        }
        Ok(Self(value))
    }

    pub fn raw(self) -> u64 {
        self.0
    }

    pub fn as_u128(self) -> u128 {
        self.0 as u128
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn checked_add(self, rhs: Shares) -> KeystoneResult<Shares> {
        self.0
            .checked_add(rhs.0)
            .map(Shares)
            .ok_or(KeystoneError::AmountOverflow)
    }

    pub fn checked_sub(self, rhs: Shares) -> KeystoneResult<Shares> {
        self.0
            .checked_sub(rhs.0)
            .map(Shares)
            .ok_or(KeystoneError::AmountUnderflow)
    }

    pub fn from_u128(value: u128) -> KeystoneResult<Shares> {
        if value > u64::MAX as u128 {
            return Err(KeystoneError::AmountOverflow);
        }
        Ok(Shares(value as u64))
    }
}

impl fmt::Display for Shares {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(
    Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Bps(pub u32);

impl Bps {
    pub const ZERO: Bps = Bps(0);
    pub const ONE_HUNDRED_PERCENT: Bps = Bps(10_000);

    pub fn new(value: u32) -> KeystoneResult<Self> {
        if value > 100_000 {
            return Err(KeystoneError::InvalidBps(value));
        }
        Ok(Self(value))
    }

    pub fn strict(value: u32) -> KeystoneResult<Self> {
        if value > 10_000 {
            return Err(KeystoneError::InvalidBps(value));
        }
        Ok(Self(value))
    }

    pub fn raw(self) -> u32 {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn apply_floor(self, amount: Amount) -> KeystoneResult<Amount> {
        Amount::from_u128(amount.as_u128() * self.0 as u128 / BPS_DENOMINATOR)
    }

    pub fn apply_ceil(self, amount: Amount) -> KeystoneResult<Amount> {
        let product = amount
            .as_u128()
            .checked_mul(self.0 as u128)
            .ok_or(KeystoneError::AmountOverflow)?;
        Amount::from_u128(product.div_ceil(BPS_DENOMINATOR))
    }

    pub fn complement(self) -> KeystoneResult<Bps> {
        if self.0 > 10_000 {
            return Err(KeystoneError::InvalidBps(self.0));
        }
        Bps::strict(10_000 - self.0)
    }

    pub fn checked_add(self, rhs: Bps) -> KeystoneResult<Bps> {
        Bps::new(
            self.0
                .checked_add(rhs.0)
                .ok_or(KeystoneError::AmountOverflow)?,
        )
    }

    pub fn checked_sub(self, rhs: Bps) -> KeystoneResult<Bps> {
        if rhs.0 > self.0 {
            return Err(KeystoneError::AmountUnderflow);
        }
        Bps::new(self.0 - rhs.0)
    }

    pub fn annualized_for_epochs(
        annual_rate: Bps,
        principal: Amount,
        elapsed_epochs: u64,
        epochs_per_year: u64,
    ) -> KeystoneResult<Amount> {
        if elapsed_epochs == 0 || annual_rate.is_zero() || principal.is_zero() {
            return Ok(Amount::ZERO);
        }
        if epochs_per_year == 0 {
            return Err(KeystoneError::DivisionByZero);
        }
        let numerator = principal
            .as_u128()
            .checked_mul(annual_rate.raw() as u128)
            .and_then(|value| value.checked_mul(elapsed_epochs as u128))
            .ok_or(KeystoneError::AmountOverflow)?;
        Amount::from_u128(numerator / BPS_DENOMINATOR / epochs_per_year as u128)
    }

    pub fn annualized_for_epochs_ceil(
        annual_rate: Bps,
        principal: Amount,
        elapsed_epochs: u64,
        epochs_per_year: u64,
    ) -> KeystoneResult<Amount> {
        if elapsed_epochs == 0 || annual_rate.is_zero() || principal.is_zero() {
            return Ok(Amount::ZERO);
        }
        if epochs_per_year == 0 {
            return Err(KeystoneError::DivisionByZero);
        }
        let numerator = principal
            .as_u128()
            .checked_mul(annual_rate.raw() as u128)
            .and_then(|value| value.checked_mul(elapsed_epochs as u128))
            .ok_or(KeystoneError::AmountOverflow)?;
        Amount::from_u128(numerator.div_ceil(BPS_DENOMINATOR * epochs_per_year as u128))
    }
}

impl fmt::Display for Bps {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}bps", self.0)
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Decimal(pub u128);

impl Decimal {
    pub const ZERO: Decimal = Decimal(0);
    pub const ONE: Decimal = Decimal(DECIMAL_SCALE);

    pub fn from_ratio(numerator: u128, denominator: u128) -> KeystoneResult<Self> {
        if denominator == 0 {
            return Err(KeystoneError::DivisionByZero);
        }
        let scaled = numerator
            .checked_mul(DECIMAL_SCALE)
            .ok_or(KeystoneError::AmountOverflow)?
            / denominator;
        Ok(Decimal(scaled))
    }

    pub fn from_amounts(numerator: Amount, denominator: Amount) -> KeystoneResult<Self> {
        Self::from_ratio(numerator.as_u128(), denominator.as_u128())
    }

    pub fn raw(self) -> u128 {
        self.0
    }

    pub fn apply_floor(self, amount: Amount) -> KeystoneResult<Amount> {
        Amount::from_u128(amount.as_u128() * self.0 / DECIMAL_SCALE)
    }

    pub fn apply_ceil(self, amount: Amount) -> KeystoneResult<Amount> {
        let product = amount
            .as_u128()
            .checked_mul(self.0)
            .ok_or(KeystoneError::AmountOverflow)?;
        Amount::from_u128(product.div_ceil(DECIMAL_SCALE))
    }

    pub fn checked_add(self, rhs: Decimal) -> KeystoneResult<Decimal> {
        self.0
            .checked_add(rhs.0)
            .map(Decimal)
            .ok_or(KeystoneError::AmountOverflow)
    }

    pub fn checked_sub(self, rhs: Decimal) -> KeystoneResult<Decimal> {
        self.0
            .checked_sub(rhs.0)
            .map(Decimal)
            .ok_or(KeystoneError::AmountUnderflow)
    }
}

pub fn shares_for_deposit(
    amount: Amount,
    nav_before: Amount,
    supply_before: Shares,
) -> KeystoneResult<Shares> {
    if amount.is_zero() {
        return Err(KeystoneError::ZeroAmount);
    }
    if supply_before.is_zero() || nav_before.is_zero() {
        return Shares::non_zero(amount.raw());
    }
    Shares::from_u128(amount.as_u128() * supply_before.as_u128() / nav_before.as_u128())
}

pub fn amount_for_shares(
    shares: Shares,
    nav_before: Amount,
    supply_before: Shares,
) -> KeystoneResult<Amount> {
    if shares.is_zero() {
        return Err(KeystoneError::ZeroShares);
    }
    if supply_before.is_zero() {
        return Err(KeystoneError::DivisionByZero);
    }
    Amount::from_u128(shares.as_u128() * nav_before.as_u128() / supply_before.as_u128())
}

pub fn split_by_shares(
    amount: Amount,
    owner_shares: Shares,
    total_shares: Shares,
) -> KeystoneResult<Amount> {
    if total_shares.is_zero() {
        return Err(KeystoneError::DivisionByZero);
    }
    Amount::from_u128(amount.as_u128() * owner_shares.as_u128() / total_shares.as_u128())
}

pub fn checked_ratio_bps(numerator: Amount, denominator: Amount) -> KeystoneResult<Bps> {
    if denominator.is_zero() {
        return Err(KeystoneError::DivisionByZero);
    }
    let value = numerator.as_u128() * BPS_DENOMINATOR / denominator.as_u128();
    Bps::new(value as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis_points_round_floor_and_ceil() {
        let rate = Bps::strict(333).unwrap();
        assert_eq!(rate.apply_floor(Amount(1000)).unwrap(), Amount(33));
        assert_eq!(rate.apply_ceil(Amount(1000)).unwrap(), Amount(34));
    }

    #[test]
    fn share_conversion_uses_current_nav() {
        let minted = shares_for_deposit(Amount(500), Amount(1_000), Shares(1_000)).unwrap();
        assert_eq!(minted, Shares(500));
        let amount = amount_for_shares(Shares(250), Amount(1_500), Shares(1_500)).unwrap();
        assert_eq!(amount, Amount(250));
    }

    #[test]
    fn annualized_interest_is_linear_by_epoch() {
        let interest =
            Bps::annualized_for_epochs(Bps::strict(1_200).unwrap(), Amount(10_000), 30, 360)
                .unwrap();
        assert_eq!(interest, Amount(100));
    }
}
