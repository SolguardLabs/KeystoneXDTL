use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{KeystoneError, KeystoneResult};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Digest(pub [u8; 32]);

macro_rules! id_type {
    ($name:ident, $domain:expr) => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(pub Digest);

        impl $name {
            pub fn from_digest(digest: Digest) -> Self {
                Self(digest)
            }

            pub fn from_parts(parts: &[&[u8]]) -> Self {
                Self(Digest::from_parts($domain, parts))
            }

            pub fn to_hex(self) -> String {
                self.0.to_hex()
            }

            pub fn bytes(self) -> [u8; 32] {
                self.0.bytes()
            }

            pub fn digest(self) -> Digest {
                self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}({})", stringify!($name), self.to_hex())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}", self.to_hex())
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.to_hex())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let text = String::deserialize(deserializer)?;
                Digest::from_hex(&text)
                    .map(Self)
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}

impl Digest {
    pub const ZERO: Digest = Digest([0u8; 32]);

    pub fn from_parts(domain: &str, parts: &[&[u8]]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(domain.as_bytes());
        hasher.update(&(parts.len() as u64).to_be_bytes());
        for part in parts {
            hasher.update(&(part.len() as u64).to_be_bytes());
            hasher.update(part);
        }
        Digest(*hasher.finalize().as_bytes())
    }

    pub fn bytes(self) -> [u8; 32] {
        self.0
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(value: &str) -> KeystoneResult<Digest> {
        let bytes =
            hex::decode(value).map_err(|error| KeystoneError::serialization(error.to_string()))?;
        if bytes.len() != 32 {
            return Err(KeystoneError::serialization("digest must have 32 bytes"));
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        Ok(Digest(out))
    }

    pub fn chain(self, domain: &str, parts: &[&[u8]]) -> Digest {
        let mut owned = Vec::with_capacity(parts.len() + 1);
        owned.push(self.0.to_vec());
        for part in parts {
            owned.push(part.to_vec());
        }
        let refs: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
        Digest::from_parts(domain, &refs)
    }

    pub fn mix_u64(self, domain: &str, value: u64) -> Digest {
        self.chain(domain, &[&value.to_be_bytes()])
    }

    pub fn mix_str(self, domain: &str, value: &str) -> Digest {
        self.chain(domain, &[value.as_bytes()])
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Digest({})", self.to_hex())
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.to_hex())
    }
}

impl Serialize for Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        Digest::from_hex(&text).map_err(serde::de::Error::custom)
    }
}

impl FromStr for Digest {
    type Err = KeystoneError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Digest::from_hex(value)
    }
}

id_type!(AccountId, "keystonexdtl-account-v1");
id_type!(AssetId, "keystonexdtl-asset-v1");
id_type!(VaultId, "keystonexdtl-vault-v1");
id_type!(LoanId, "keystonexdtl-loan-v1");
id_type!(TxId, "keystonexdtl-tx-v1");
id_type!(PositionId, "keystonexdtl-position-v1");

impl AccountId {
    pub fn named(name: &str) -> Self {
        Self::from_parts(&[name.as_bytes()])
    }
}

impl AssetId {
    pub fn native() -> Self {
        Self::from_parts(&[b"usdc-dtl"])
    }

    pub fn named(symbol: &str) -> Self {
        Self::from_parts(&[symbol.as_bytes()])
    }
}

impl VaultId {
    pub fn named(name: &str, asset: AssetId) -> Self {
        Self::from_parts(&[name.as_bytes(), &asset.bytes()])
    }
}

impl LoanId {
    pub fn derive(
        lender: VaultId,
        borrower: VaultId,
        nonce: u64,
        principal: u64,
        start_epoch: u64,
    ) -> Self {
        Self::from_parts(&[
            &lender.bytes(),
            &borrower.bytes(),
            &nonce.to_be_bytes(),
            &principal.to_be_bytes(),
            &start_epoch.to_be_bytes(),
        ])
    }
}

impl TxId {
    pub fn derive(kind: &str, sequence: u64, state_digest: Digest) -> Self {
        Self::from_parts(&[
            kind.as_bytes(),
            &sequence.to_be_bytes(),
            &state_digest.bytes(),
        ])
    }
}

impl PositionId {
    pub fn derive(vault: VaultId, owner: AccountId) -> Self {
        Self::from_parts(&[&vault.bytes(), &owner.bytes()])
    }
}

pub fn state_digest_from_json<T: Serialize>(domain: &str, value: &T) -> KeystoneResult<Digest> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| KeystoneError::serialization(error.to_string()))?;
    Ok(Digest::from_parts(domain, &[&bytes]))
}

pub fn digest_pair(domain: &str, left: Digest, right: Digest) -> Digest {
    Digest::from_parts(domain, &[&left.bytes(), &right.bytes()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_hex_encoded() {
        let account = AccountId::named("alice");
        assert_eq!(account.to_hex().len(), 64);
        assert_ne!(account, AccountId::named("bob"));
    }

    #[test]
    fn digest_deserializes_from_hex() {
        let digest = Digest::from_parts("test", &[b"a"]);
        let encoded = serde_json::to_string(&digest).unwrap();
        let decoded: Digest = serde_json::from_str(&encoded).unwrap();
        assert_eq!(digest, decoded);
    }

    #[test]
    fn loan_ids_bind_participants_and_nonce() {
        let asset = AssetId::native();
        let lender = VaultId::named("lender", asset);
        let borrower = VaultId::named("borrower", asset);
        let first = LoanId::derive(lender, borrower, 1, 100, 5);
        let second = LoanId::derive(lender, borrower, 2, 100, 5);
        assert_ne!(first, second);
    }
}
