use std::future;

use argon2::{self, Config};
use chrono::prelude::*;
use jsonwebtoken::{encode, decode, Header, Algorithm, EncodingKey, DecodingKey, Validation};
use rand::Rng;

use warp::http::StatusCode;
use warp::Filter;

use crate::store::Store;
use crate::types::account::{Account, AccountId, Session, Claims};


pub async fn register(store: Store, account: Account) -> Result<impl warp::Reply, warp::Rejection> {
    let hash_password = hash_password(account.password.as_bytes());

    let account = Account {
        id: account.id,
        email: account.email,
        password: hash_password,
    };

    match store.add_account(account).await {
        Ok(_) => Ok(warp::reply::with_status("Account added", StatusCode::OK)),
        Err(e) => Err(warp::reject::custom(e)),
    }
}

pub fn hash_password(password: &[u8]) -> String {
    let salt = rand::thread_rng().gen::<[u8; 32]>();
    let config = Config::default();
    argon2::hash_encoded(password, &salt, &config).unwrap()
}

pub async fn login(
    store: Store,
    login: Account,
) -> Result<impl warp::Reply, warp::Rejection> {
    match store.get_account(login.email).await {
        Ok(account) => match verify_password(
            &account.password,
            login.password.as_bytes()
        ) {
            Ok(verified) => {
                if verified {
                    Ok(warp::reply::json(&issue_token(
                        account.id.expect("id not found"),
                    )))
                } else {
                    Err(warp::reject::custom(handle_errors::Error::WrongPassword))
                }
            },
            Err(e) => Err(warp::reject::custom(
                handle_errors::Error::ArgonLibraryError(e),
            )),
        },
        Err(e) => Err(warp::reject::custom(e)),
    }
}

fn verify_password(hash: &str, password: &[u8]) -> Result<bool, argon2::Error> {
    argon2::verify_encoded(hash, password)
}

fn issue_token(account_id: AccountId) -> String {
    let now = Utc::now();
    let exp = now + chrono::Duration::days(1);

    let claims = Claims {
        account_id: account_id.0,  // 提取 i32 值
        exp: exp.timestamp(),
        iat: now.timestamp(),
        nbf: now.timestamp(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret("RANDOM WORDS WINTER MACINTOSH PC AAAAA".as_bytes()),
    ).expect("Failed to create token")
}

pub fn verify_token(token: String) -> Result<Session, handle_errors::Error> {
    let token_data = decode::<Claims>(
        &token,
        &DecodingKey::from_secret("RANDOM WORDS WINTER MACINTOSH PC AAAAA".as_bytes()),
        &Validation::new(Algorithm::HS256),
    ).map_err(|_| handle_errors::Error::CannotDecryptToken)?;

    Ok(Session {
        account_id: AccountId(token_data.claims.account_id),
        exp: Utc.timestamp_opt(token_data.claims.exp, 0).unwrap(),
        nbf: Utc.timestamp_opt(token_data.claims.nbf, 0).unwrap(),
    })
}

pub fn auth() -> impl Filter<Extract = (Session,), Error = warp::Rejection> + Clone {
    warp::header::<String>("Authorization").and_then(|token: String| {
        let token = match verify_token(token) {
            Ok(t) => t,
            Err(_) => return future::ready(Err(warp::reject::reject())),
        };

        future::ready(Ok(token))
    })
}