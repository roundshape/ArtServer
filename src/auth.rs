//! Bearer トークン認証。認証に通った要求には [`Principal`] を付けて先へ渡す。

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

/// 認証済みの主体。作業セッションのハンドルはこの主体に結びつく。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Principal(Arc<str>);

impl Principal {
    pub fn new(name: &str) -> Self {
        Self(name.into())
    }
}

/// 受け付けるトークンと、それに対応する主体。
#[derive(Clone)]
pub struct Auth {
    token: Arc<[u8]>,
    principal: Principal,
}

impl Auth {
    /// トークン1つを受け付ける。主体は今のところ1つだけ（複数ユーザー対応は Phase 3）。
    pub fn new(token: &str) -> Self {
        Self {
            token: token.as_bytes().into(),
            principal: Principal::new("default"),
        }
    }

    fn verify(&self, presented: &str) -> Option<Principal> {
        bool::from(self.token.ct_eq(presented.as_bytes())).then(|| self.principal.clone())
    }
}

/// `Authorization: Bearer <token>` を検査するミドルウェア。
pub async fn require_bearer(State(auth): State<Auth>, mut req: Request, next: Next) -> Response {
    let principal = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .and_then(|(_, token)| auth.verify(token.trim()));
    match principal {
        Some(p) => {
            req.extensions_mut().insert(p);
            next.run(req).await
        }
        None => (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))],
            "unauthorized",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_exact_token_is_accepted() {
        let auth = Auth::new("correct-horse-battery-staple-0123456789");
        assert!(auth.verify("correct-horse-battery-staple-0123456789").is_some());
        assert!(auth.verify("correct-horse-battery-staple-012345678").is_none());
        assert!(auth.verify("").is_none());
    }
}
