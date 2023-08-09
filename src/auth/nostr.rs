use super::AuthError;
use crate::{now, sha256, AppState, Error, Result};
use actix_web::http::header::AUTHORIZATION;
use actix_web::{dev::Payload, http::Uri, web, FromRequest, HttpRequest};
use base64::engine::{general_purpose, Engine};
use nostr_sdk::nostr::Event;
use serde::de::DeserializeOwned;
use std::{future::Future, pin::Pin};

#[derive(Debug)]
pub struct NostrAuth {
    pub pubkey: Vec<u8>,
    pub url: Uri,
    pub method: String,
    pub payload_sha: Option<Vec<u8>>,
    pub created_at: i64,
    /// real payload
    pub payload: Vec<u8>,
}

impl NostrAuth {
    pub fn verify_time(&self, diff_seconds: u64) -> Result<(), AuthError> {
        if (now() as i64 - self.created_at).abs() > diff_seconds as i64 {
            Err(AuthError::InvalidEvent(
                "Invalid nostr event, timestamp out of range",
            ))
        } else {
            Ok(())
        }
    }
    pub fn verify_http(&self, url: &Uri, method: &str) -> Result<(), AuthError> {
        if url != &self.url {
            return Err(AuthError::InvalidEvent("Invalid nostr event, invalid url"));
        }
        if method != self.method {
            return Err(AuthError::InvalidEvent(
                "Invalid nostr event, invalid method",
            ));
        }

        if method == "POST" || method == "PUT" || method == "PATCH" {
            match &self.payload_sha {
                Some(sha) => {
                    if &sha256(&self.payload) != sha {
                        return Err(AuthError::InvalidEvent(
                            "Invalid nostr event, invalid payload",
                        ));
                    }
                }
                None => {
                    return Err(AuthError::InvalidEvent(
                        "Invalid nostr event, missing payload",
                    ));
                }
            }
        }
        Ok(())
    }

    fn from_token(s: &str, payload: Vec<u8>) -> Result<Self, AuthError> {
        let buf = general_purpose::STANDARD.decode(s)?;
        let json = String::from_utf8(buf)?;

        let event = Event::from_json(json)?;

        if event.kind.as_u32() != 27_235 {
            return Err(AuthError::InvalidEvent("Invalid nostr event, wrong kind"));
        }

        let mut url = None;
        let mut method = None;
        let mut payload_sha = None;

        for tag in &event.tags {
            let tag = tag.as_vec();
            if tag.len() > 1 {
                match tag[0].as_str() {
                    "u" => {
                        url = Some(tag[1].clone().parse().map_err(|_| {
                            AuthError::InvalidEvent("Invalid nostr event, invalid url")
                        })?)
                    }
                    "method" => method = Some(tag[1].clone()),
                    "payload" => {
                        payload_sha = Some(hex::decode(&tag[1]).map_err(|_| {
                            AuthError::InvalidEvent("Invalid nostr event, invalid payload")
                        })?)
                    }
                    _ => {}
                }
            }
        }
        if url.is_none() {
            return Err(AuthError::InvalidEvent("Invalid nostr event, missing url"));
        }
        if method.is_none() {
            return Err(AuthError::InvalidEvent(
                "Invalid nostr event, missing method",
            ));
        }

        Ok(Self {
            pubkey: event.pubkey.serialize().to_vec(),
            url: url.unwrap(),
            method: method.unwrap(),
            payload_sha,
            created_at: event.created_at.as_i64(),
            payload,
        })
    }
}

impl FromRequest for NostrAuth {
    type Error = Error;
    // type Future = Ready<Result<NostrUser>>;
    type Future = Pin<Box<dyn Future<Output = Result<NostrAuth>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let req = req.clone();
        let mut payload = payload.take();

        Box::pin(async move {
            if let Some(auth) = req.headers().get(AUTHORIZATION) {
                if let Ok(auth) = auth.to_str() {
                    if let Some(_state) = req.app_data::<web::Data<AppState>>() {
                        if auth.starts_with("Nostr") || auth.starts_with("nostr") {
                            let bytes = web::Bytes::from_request(&req, &mut payload)
                                .await
                                .map_err(|e| Error::Message(e.to_string()))?;

                            // println!("auth: {:?} {:?} {:?}", bytes, req.uri(), req.method());

                            let token = auth[5..auth.len()].trim();
                            let user = NostrAuth::from_token(token, bytes.to_vec())?;
                            user.verify_time(60)?;
                            user.verify_http(req.uri(), req.method().as_str())?;
                            return Ok(user);
                        }
                    } else {
                        return Err(Error::Str("AppState required"));
                    }
                }
            }
            Err(AuthError::InvalidEvent("Invalid nostr event, missing auth").into())
        })
    }
}

/// Nostr auth data and json payload
#[derive(Debug)]
pub struct Json<T> {
    pub auth: NostrAuth,
    pub data: T,
}

impl<T: DeserializeOwned> FromRequest for Json<T> {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Json<T>, Error>>>>;
    fn from_request(req: &HttpRequest, pl: &mut Payload) -> Self::Future {
        let fut = NostrAuth::from_request(req, pl);
        Box::pin(async move {
            let user = fut.await?;
            //form: serde_urlencoded::from_bytes::<T>(&body).map_err(UrlencodedError::Parse)
            let data = serde_json::from_slice::<T>(&user.payload)?;
            Ok(Json { auth: user, data })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::{secp256k1::SecretKey, EventBuilder, Keys, Kind, Tag};
    use std::{str::FromStr, time::Duration};

    const ALICE_SK: &str = "6b911fd37cdf5c81d4c0adb1ab7fa822ed253ab0ad9aa18d77257c88b29b718e";

    #[tokio::test]
    async fn parse() -> anyhow::Result<()> {
        let secret_key = SecretKey::from_str(ALICE_SK)?;
        let alice_keys = Keys::new(secret_key);
        let event = EventBuilder::new(
            Kind::from(27235),
            "",
            &vec![
                Tag::try_from(vec!["u", "url"])?,
                Tag::try_from(vec!["method", "GET"])?,
            ],
        )
        .to_event(&alice_keys)?;

        let encoded = general_purpose::STANDARD.encode(event.as_json());

        let user: NostrAuth = NostrAuth::from_token(&encoded, vec![])?;
        user.verify_time(60)?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(user.verify_time(1).is_err());
        user.verify_http(&"url".parse()?, "GET")?;

        assert!(user.verify_http(&"url1".parse()?, "GET").is_err());
        assert!(user.verify_http(&"url".parse()?, "POST").is_err());

        let body = b"{}".to_vec();
        let event = EventBuilder::new(
            Kind::from(27235),
            "",
            &vec![
                Tag::try_from(vec!["u", "url"])?,
                Tag::try_from(vec!["method", "POST"])?,
                Tag::try_from(vec!["payload", &hex::encode(sha256(&body))])?,
            ],
        )
        .to_event(&alice_keys)?;

        let encoded = general_purpose::STANDARD.encode(event.as_json());

        let user: NostrAuth = NostrAuth::from_token(&encoded, body)?;
        assert_eq!(user.pubkey, alice_keys.public_key().serialize());

        user.verify_http(&"url".parse()?, "POST")?;
        assert!(user.verify_http(&"url1".parse()?, "GET").is_err());
        assert!(user.verify_http(&"url1".parse()?, "POST").is_err());
        Ok(())
    }
}
