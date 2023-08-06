use crate::{now, AppState, Error, Result};
use actix_http::header::AUTHORIZATION;
use actix_web::{dev::Payload, web, FromRequest, HttpRequest};
use entity::user;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};

#[derive(Serialize, Deserialize, Debug)]
pub struct AuthToken {
    // issued at
    pub iat: i64,
    // expiration
    pub exp: i64,
    // data
    pub user_id: i64,
}

impl AuthToken {
    pub fn from_str(token: &str, secret: &[u8]) -> Result<Self> {
        let mut validation = Validation::default();
        validation.leeway = 0;
        Ok(jsonwebtoken::decode::<AuthToken>(
            token,
            &DecodingKey::from_secret(secret),
            &validation,
        )?
        .claims)
    }

    pub fn generate(user_id: i64, expiry: usize, secret: &[u8]) -> Result<String> {
        let now = now() as i64;
        let payload = AuthToken {
            iat: now,
            exp: now + expiry as i64,
            user_id,
        };

        Ok(jsonwebtoken::encode(
            &Header::default(),
            &payload,
            &EncodingKey::from_secret(secret),
        )?)
    }
}

#[derive(Debug)]
pub struct AuthedUser {
    pub user: user::Model,
}

impl AuthedUser {
    pub async fn from_token(token: &str, state: &AppState) -> Result<Self> {
        let token = AuthToken::from_str(token, state.setting.auth.secret.as_bytes())?;
        let user = state.service.get_user_by_id(token.user_id).await?;
        Ok(Self { user })
    }
}

impl FromRequest for AuthedUser {
    type Error = Error;
    // type Future = Ready<Result<AuthedUser>>;
    type Future = Pin<Box<dyn Future<Output = Result<AuthedUser>>>>;

    fn from_request(req: &HttpRequest, _pl: &mut Payload) -> Self::Future {
        let req = req.clone();
        Box::pin(async move {
            if let Some(state) = req.app_data::<web::Data<AppState>>() {
                if let Some(auth) = req.headers().get(AUTHORIZATION) {
                    if let Ok(auth) = auth.to_str() {
                        if auth.starts_with("bearer") || auth.starts_with("Bearer") {
                            let token = auth[6..auth.len()].trim();

                            return AuthedUser::from_token(token, state).await;
                        }
                    }
                }
            }
            Err(Error::Unauthorized)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn token() -> Result<()> {
        let token = AuthToken::generate(1, 3600, b"secret")?;
        let auth = AuthToken::from_str(&token, b"secret")?;
        assert_eq!(auth.user_id, 1);
        // expired
        let token = AuthToken::generate(1, 1, b"secret")?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        let res = AuthToken::from_str(&token, b"secret");
        assert!(res.is_err());
        Ok(())
    }
}
