use serde::{Deserialize, Serialize};

use crate::{AccountId, Amount, Digest, Epoch, KeystoneResult, LoanId, Shares, TxId, VaultId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    EngineInitialized {
        network_id: u32,
        epoch: Epoch,
    },
    VaultRegistered {
        vault: VaultId,
        role: String,
    },
    AccountDeposited {
        vault: VaultId,
        owner: AccountId,
        amount: Amount,
        shares: Shares,
    },
    AccountRedeemed {
        vault: VaultId,
        owner: AccountId,
        amount: Amount,
        shares: Shares,
    },
    LoanOpened {
        loan: LoanId,
        lender: VaultId,
        borrower: VaultId,
        principal: Amount,
        projected_interest: Amount,
    },
    LoanRepaid {
        loan: LoanId,
        payer: VaultId,
        principal: Amount,
        interest: Amount,
        mode: String,
    },
    CollateralReleased {
        loan: LoanId,
        borrower: VaultId,
        amount: Amount,
    },
    LoanDefaulted {
        loan: LoanId,
        overdue_epochs: u64,
    },
    LoanLiquidated {
        loan: LoanId,
        seized: Amount,
        shortfall: Amount,
    },
    InterestDistributed {
        vault: VaultId,
        amount: Amount,
        recipients: usize,
    },
    EpochAdvanced {
        from: Epoch,
        to: Epoch,
    },
    PolicyUpdated {
        digest: Digest,
    },
    InvariantChecked {
        digest: Digest,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub sequence: u64,
    pub tx: TxId,
    pub epoch: Epoch,
    pub state_digest: Digest,
    pub kind: EventKind,
}

impl Event {
    pub fn new(
        sequence: u64,
        epoch: Epoch,
        previous_digest: Digest,
        kind: EventKind,
    ) -> KeystoneResult<Self> {
        let kind_bytes = serde_json::to_vec(&kind)
            .map_err(|error| crate::KeystoneError::serialization(error.to_string()))?;
        let state_digest = previous_digest.chain(
            "keystonexdtl-event-state-v1",
            &[
                &sequence.to_be_bytes(),
                &epoch.raw().to_be_bytes(),
                &kind_bytes,
            ],
        );
        let tx = TxId::derive("event", sequence, state_digest);
        Ok(Self {
            sequence,
            tx,
            epoch,
            state_digest,
            kind,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Journal {
    events: Vec<Event>,
    digest: Digest,
}

impl Journal {
    pub fn new(network_id: u32, epoch: Epoch) -> KeystoneResult<Self> {
        let initial = Digest::from_parts(
            "keystonexdtl-journal-genesis-v1",
            &[&network_id.to_be_bytes(), &epoch.raw().to_be_bytes()],
        );
        let mut journal = Self {
            events: Vec::new(),
            digest: initial,
        };
        journal.push(epoch, EventKind::EngineInitialized { network_id, epoch })?;
        Ok(journal)
    }

    pub fn push(&mut self, epoch: Epoch, kind: EventKind) -> KeystoneResult<TxId> {
        let sequence = self.events.len() as u64;
        let event = Event::new(sequence, epoch, self.digest, kind)?;
        self.digest = event.state_digest;
        let tx = event.tx;
        self.events.push(event);
        Ok(tx)
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn digest(&self) -> Digest {
        self.digest
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn last(&self) -> Option<&Event> {
        self.events.last()
    }

    pub fn by_loan(&self, loan: LoanId) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|event| match &event.kind {
                EventKind::LoanOpened { loan: id, .. }
                | EventKind::LoanRepaid { loan: id, .. }
                | EventKind::CollateralReleased { loan: id, .. }
                | EventKind::LoanDefaulted { loan: id, .. }
                | EventKind::LoanLiquidated { loan: id, .. } => *id == loan,
                _ => false,
            })
            .collect()
    }

    pub fn by_vault(&self, vault: VaultId) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|event| match &event.kind {
                EventKind::VaultRegistered { vault: id, .. }
                | EventKind::AccountDeposited { vault: id, .. }
                | EventKind::AccountRedeemed { vault: id, .. }
                | EventKind::InterestDistributed { vault: id, .. } => *id == vault,
                EventKind::LoanOpened {
                    lender, borrower, ..
                } => *lender == vault || *borrower == vault,
                EventKind::LoanRepaid { payer, .. } => *payer == vault,
                EventKind::CollateralReleased { borrower, .. } => *borrower == vault,
                _ => false,
            })
            .collect()
    }

    pub fn replay_digest(&self, network_id: u32, epoch: Epoch) -> KeystoneResult<Digest> {
        let mut digest = Digest::from_parts(
            "keystonexdtl-journal-genesis-v1",
            &[&network_id.to_be_bytes(), &epoch.raw().to_be_bytes()],
        );
        for event in &self.events {
            let kind_bytes = serde_json::to_vec(&event.kind)
                .map_err(|error| crate::KeystoneError::serialization(error.to_string()))?;
            digest = digest.chain(
                "keystonexdtl-event-state-v1",
                &[
                    &event.sequence.to_be_bytes(),
                    &event.epoch.raw().to_be_bytes(),
                    &kind_bytes,
                ],
            );
        }
        Ok(digest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_digest_changes_per_event() {
        let epoch = Epoch(1);
        let mut journal = Journal::new(7, epoch).unwrap();
        let before = journal.digest();
        journal
            .push(
                epoch,
                EventKind::PolicyUpdated {
                    digest: Digest::from_parts("policy", &[b"a"]),
                },
            )
            .unwrap();
        assert_ne!(before, journal.digest());
        assert_eq!(journal.len(), 2);
    }
}
