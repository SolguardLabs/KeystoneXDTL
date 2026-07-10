use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Amount, AssetId, Bps, Digest, Epoch, KeystoneError, KeystoneResult};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Price {
    pub asset: AssetId,
    pub quote_asset: AssetId,
    pub price_e9: u128,
    pub confidence_bps: Bps,
    pub published_epoch: Epoch,
}

impl Price {
    pub fn new(
        asset: AssetId,
        quote_asset: AssetId,
        price_e9: u128,
        confidence_bps: Bps,
        published_epoch: Epoch,
    ) -> KeystoneResult<Self> {
        if price_e9 == 0 {
            return Err(KeystoneError::PriceNotAvailable);
        }
        Ok(Self {
            asset,
            quote_asset,
            price_e9,
            confidence_bps,
            published_epoch,
        })
    }

    pub fn one(asset: AssetId, epoch: Epoch) -> KeystoneResult<Self> {
        Self::new(asset, asset, 1_000_000_000, Bps::strict(20)?, epoch)
    }

    pub fn convert_floor(self, amount: Amount) -> KeystoneResult<Amount> {
        Amount::from_u128(amount.as_u128() * self.price_e9 / 1_000_000_000)
    }

    pub fn convert_ceil(self, amount: Amount) -> KeystoneResult<Amount> {
        let product = amount
            .as_u128()
            .checked_mul(self.price_e9)
            .ok_or(KeystoneError::AmountOverflow)?;
        Amount::from_u128(product.div_ceil(1_000_000_000))
    }

    pub fn checked_fresh(self, now: Epoch, max_age_epochs: u64) -> KeystoneResult<Self> {
        let age = now.elapsed_since(self.published_epoch)?;
        if age > max_age_epochs {
            return Err(KeystoneError::StalePrice);
        }
        Ok(self)
    }

    pub fn conservative_value(self, amount: Amount) -> KeystoneResult<Amount> {
        let converted = self.convert_floor(amount)?;
        let haircut = self.confidence_bps.apply_ceil(converted)?;
        converted.checked_sub(haircut)
    }

    pub fn digest(self) -> KeystoneResult<Digest> {
        let bytes = serde_json::to_vec(&self)
            .map_err(|error| KeystoneError::serialization(error.to_string()))?;
        Ok(Digest::from_parts("keystonexdtl-price-v1", &[&bytes]))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OracleBook {
    prices: BTreeMap<(AssetId, AssetId), Price>,
    max_age_epochs: u64,
}

impl OracleBook {
    pub fn new(max_age_epochs: u64) -> Self {
        Self {
            prices: BTreeMap::new(),
            max_age_epochs,
        }
    }

    pub fn insert(&mut self, price: Price) {
        self.prices.insert((price.asset, price.quote_asset), price);
    }

    pub fn set_unit_price(&mut self, asset: AssetId, epoch: Epoch) -> KeystoneResult<()> {
        let price = Price::one(asset, epoch)?;
        self.insert(price);
        Ok(())
    }

    pub fn get(&self, asset: AssetId, quote: AssetId, now: Epoch) -> KeystoneResult<Price> {
        if asset == quote {
            return Price::one(asset, now);
        }
        self.prices
            .get(&(asset, quote))
            .copied()
            .ok_or(KeystoneError::PriceNotAvailable)?
            .checked_fresh(now, self.max_age_epochs)
    }

    pub fn convert_floor(
        &self,
        asset: AssetId,
        quote: AssetId,
        amount: Amount,
        now: Epoch,
    ) -> KeystoneResult<Amount> {
        self.get(asset, quote, now)?.convert_floor(amount)
    }

    pub fn conservative_value(
        &self,
        asset: AssetId,
        quote: AssetId,
        amount: Amount,
        now: Epoch,
    ) -> KeystoneResult<Amount> {
        self.get(asset, quote, now)?.conservative_value(amount)
    }

    pub fn digest(&self) -> KeystoneResult<Digest> {
        let entries: Vec<Price> = self.prices.values().copied().collect();
        let bytes = serde_json::to_vec(&(self.max_age_epochs, entries))
            .map_err(|error| KeystoneError::serialization(error.to_string()))?;
        Ok(Digest::from_parts("keystonexdtl-oracle-book-v1", &[&bytes]))
    }

    pub fn len(&self) -> usize {
        self.prices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.prices.is_empty()
    }
}

impl Default for OracleBook {
    fn default() -> Self {
        Self::new(24)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_price_converts_identity() {
        let asset = AssetId::native();
        let mut book = OracleBook::default();
        book.set_unit_price(asset, Epoch(1)).unwrap();
        assert_eq!(
            book.convert_floor(asset, asset, Amount(700), Epoch(2))
                .unwrap(),
            Amount(700)
        );
    }
}
