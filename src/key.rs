use nostr_sdk::{
    prelude::{FromPkStr, FromSkStr},
    secp256k1::{SecretKey, XOnlyPublicKey},
    Keys,
};
use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{fmt, ops::Deref};

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(into = "SecretKey")]
pub struct Privkey(SecretKey);

impl From<SecretKey> for Privkey {
    fn from(val: SecretKey) -> Self {
        Privkey(val)
    }
}

impl From<Privkey> for SecretKey {
    fn from(val: Privkey) -> Self {
        val.0
    }
}

impl Deref for Privkey {
    type Target = SecretKey;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Privkey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(PrivkeyVisitor)
    }
}

struct PrivkeyVisitor;

impl<'de> Visitor<'de> for PrivkeyVisitor {
    type Value = Privkey;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("struct Privkey")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Keys::from_sk_str(v)
            .map(|k| Privkey(k.secret_key().unwrap()))
            .map_err(Error::custom)
    }
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(into = "XOnlyPublicKey")]
pub struct Pubkey(XOnlyPublicKey);

impl From<XOnlyPublicKey> for Pubkey {
    fn from(val: XOnlyPublicKey) -> Self {
        Pubkey(val)
    }
}

impl From<Pubkey> for XOnlyPublicKey {
    fn from(val: Pubkey) -> Self {
        val.0
    }
}

impl Deref for Pubkey {
    type Target = XOnlyPublicKey;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Pubkey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(PubkeyVisitor)
    }
}

struct PubkeyVisitor;

impl<'de> Visitor<'de> for PubkeyVisitor {
    type Value = Pubkey;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("struct Pubkey")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Keys::from_pk_str(v)
            .map(|k| Pubkey(k.public_key()))
            .map_err(Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn der() -> Result<()> {
        // npub1fuvh5hz9tvyesqnrsrjlfy45j9dwj0zrzuzs4jy53kff850ge5sq6te9w6
        // nsec1cfnu2t9xpdxk25ufrtfqrm4a5whjrtwuakmzha3ye9pyzwsva4rqr0d73w
        // 4f197a5c455b0998026380e5f492b4915ae93c4317050ac8948d9293d1e8cd20
        // c267c52ca60b4d6553891ad201eebda3af21addcedb62bf624c942413a0ced46
        let privkey: Privkey = serde_json::from_str(
            "\"nsec1cfnu2t9xpdxk25ufrtfqrm4a5whjrtwuakmzha3ye9pyzwsva4rqr0d73w\"",
        )?;
        assert_eq!(
            privkey.as_ref().to_vec(),
            hex::decode("c267c52ca60b4d6553891ad201eebda3af21addcedb62bf624c942413a0ced46")?
        );

        let privkey: Privkey = serde_json::from_str(
            "\"c267c52ca60b4d6553891ad201eebda3af21addcedb62bf624c942413a0ced46\"",
        )?;
        assert_eq!(
            privkey.as_ref().to_vec(),
            hex::decode("c267c52ca60b4d6553891ad201eebda3af21addcedb62bf624c942413a0ced46")?
        );

        let pubkey: Pubkey = serde_json::from_str(
            "\"npub1fuvh5hz9tvyesqnrsrjlfy45j9dwj0zrzuzs4jy53kff850ge5sq6te9w6\"",
        )?;
        assert_eq!(
            pubkey.deref().serialize().to_vec(),
            hex::decode("4f197a5c455b0998026380e5f492b4915ae93c4317050ac8948d9293d1e8cd20")?
        );

        let pubkey: Pubkey = serde_json::from_str(
            "\"4f197a5c455b0998026380e5f492b4915ae93c4317050ac8948d9293d1e8cd20\"",
        )?;
        assert_eq!(
            pubkey.deref().serialize().to_vec(),
            hex::decode("4f197a5c455b0998026380e5f492b4915ae93c4317050ac8948d9293d1e8cd20")?
        );

        Ok(())
    }
}
