use serde::{Deserialize, Serialize};

use crate::{KeystoneError, KeystoneResult};

#[derive(
    Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Epoch(pub u64);

impl Epoch {
    pub const ZERO: Epoch = Epoch(0);

    pub fn new(value: u64) -> Self {
        Epoch(value)
    }

    pub fn raw(self) -> u64 {
        self.0
    }

    pub fn checked_add(self, rhs: u64) -> KeystoneResult<Epoch> {
        self.0
            .checked_add(rhs)
            .map(Epoch)
            .ok_or(KeystoneError::AmountOverflow)
    }

    pub fn checked_sub(self, rhs: Epoch) -> KeystoneResult<u64> {
        self.0.checked_sub(rhs.0).ok_or(KeystoneError::InvalidEpoch)
    }

    pub fn elapsed_since(self, start: Epoch) -> KeystoneResult<u64> {
        self.checked_sub(start)
    }

    pub fn is_after(self, other: Epoch) -> bool {
        self.0 > other.0
    }

    pub fn is_at_or_after(self, other: Epoch) -> bool {
        self.0 >= other.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub start: Epoch,
    pub maturity: Epoch,
    pub grace_epochs: u64,
    pub epochs_per_year: u64,
}

impl Schedule {
    pub fn new(
        start: Epoch,
        tenor_epochs: u64,
        grace_epochs: u64,
        epochs_per_year: u64,
    ) -> KeystoneResult<Self> {
        if tenor_epochs == 0 || epochs_per_year == 0 {
            return Err(KeystoneError::InvalidTenor);
        }
        Ok(Self {
            start,
            maturity: start.checked_add(tenor_epochs)?,
            grace_epochs,
            epochs_per_year,
        })
    }

    pub fn tenor_epochs(self) -> KeystoneResult<u64> {
        self.maturity.checked_sub(self.start)
    }

    pub fn elapsed(self, now: Epoch) -> KeystoneResult<u64> {
        let maturity_elapsed = self.tenor_epochs()?;
        if now <= self.start {
            return Ok(0);
        }
        let elapsed = now.checked_sub(self.start)?;
        Ok(elapsed.min(maturity_elapsed))
    }

    pub fn remaining(self, now: Epoch) -> KeystoneResult<u64> {
        let tenor = self.tenor_epochs()?;
        Ok(tenor.saturating_sub(self.elapsed(now)?))
    }

    pub fn due_epoch(self) -> KeystoneResult<Epoch> {
        self.maturity.checked_add(self.grace_epochs)
    }

    pub fn is_mature(self, now: Epoch) -> bool {
        now.is_at_or_after(self.maturity)
    }

    pub fn is_overdue(self, now: Epoch) -> KeystoneResult<bool> {
        Ok(now.is_after(self.due_epoch()?))
    }

    pub fn progress_bps(self, now: Epoch) -> KeystoneResult<u32> {
        let tenor = self.tenor_epochs()?;
        let elapsed = self.elapsed(now)?;
        Ok(((elapsed as u128) * 10_000 / tenor as u128) as u32)
    }

    pub fn checkpoint_epochs(self, count: u64) -> KeystoneResult<Vec<Epoch>> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let tenor = self.tenor_epochs()?;
        let mut out = Vec::with_capacity(count as usize);
        for index in 1..=count {
            let offset = tenor * index / count;
            out.push(self.start.checked_add(offset)?);
        }
        Ok(out)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochWindow {
    pub open: Epoch,
    pub close: Epoch,
}

impl EpochWindow {
    pub fn new(open: Epoch, close: Epoch) -> KeystoneResult<Self> {
        if close <= open {
            return Err(KeystoneError::InvalidEpoch);
        }
        Ok(Self { open, close })
    }

    pub fn contains(self, now: Epoch) -> bool {
        now >= self.open && now < self.close
    }

    pub fn width(self) -> KeystoneResult<u64> {
        self.close.checked_sub(self.open)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EpochClock {
    current: Epoch,
    last_tick_digest: crate::Digest,
}

impl EpochClock {
    pub fn new(start: Epoch) -> Self {
        Self {
            current: start,
            last_tick_digest: crate::Digest::from_parts(
                "keystonexdtl-clock-v1",
                &[&start.raw().to_be_bytes()],
            ),
        }
    }

    pub fn current(&self) -> Epoch {
        self.current
    }

    pub fn advance_to(&mut self, target: Epoch) -> KeystoneResult<()> {
        if target < self.current {
            return Err(KeystoneError::InvalidEpoch);
        }
        self.current = target;
        self.last_tick_digest = self
            .last_tick_digest
            .mix_u64("keystonexdtl-clock-advance-v1", target.raw());
        Ok(())
    }

    pub fn advance_by(&mut self, epochs: u64) -> KeystoneResult<Epoch> {
        let target = self.current.checked_add(epochs)?;
        self.advance_to(target)?;
        Ok(target)
    }

    pub fn digest(&self) -> crate::Digest {
        self.last_tick_digest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_caps_elapsed_at_maturity() {
        let schedule = Schedule::new(Epoch(10), 30, 5, 360).unwrap();
        assert_eq!(schedule.elapsed(Epoch(12)).unwrap(), 2);
        assert_eq!(schedule.elapsed(Epoch(100)).unwrap(), 30);
        assert!(schedule.is_overdue(Epoch(46)).unwrap());
    }

    #[test]
    fn clock_advances_forward_only() {
        let mut clock = EpochClock::new(Epoch(5));
        assert_eq!(clock.advance_by(7).unwrap(), Epoch(12));
        assert!(clock.advance_to(Epoch(11)).is_err());
    }
}
