use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use common::config::OidcConfig;
use openidconnect::{
    AccessTokenHash, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet,
    EndpointNotSet, EndpointSet, IssuerUrl, Nonce, OAuth2TokenResponse, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
    reqwest,
};
use openssl::rand::rand_bytes;
use serde_json::Value;
use thiserror::Error;
use tokio::sync::Mutex;
use url::Url;

const OIDC_RANDOM_BYTES: usize = 32;
const PENDING_LOGIN_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_PENDING_LOGINS: usize = 1024;

type DiscoveredCoreClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OidcError {
    #[error("openid connect login is not configured")]
    NotConfigured,
    #[error("openid connect login state is invalid")]
    InvalidState,
    #[error("openid connect login state expired")]
    ExpiredState,
    #[error("openid connect username claim is missing or invalid")]
    MissingUsernameClaim,
    #[error("openid connect user does not exist")]
    MissingUser,
    #[error("openid connect username is already owned by another login method or identity")]
    UsernameCollision,
    #[error("openid connect return target is invalid")]
    InvalidReturnTarget,
    #[error("openid connect provider request failed")]
    ProviderRequest,
    #[error("openid connect token response is invalid")]
    InvalidTokenResponse,
    #[error("openid connect id token is missing")]
    MissingIdToken,
    #[error("openid connect id token is invalid")]
    InvalidIdToken,
    #[error("openid connect access token hash is invalid")]
    InvalidAccessTokenHash,
}

#[derive(Debug, Clone)]
pub struct PendingOidcLogin {
    pub nonce: String,
    pub code_verifier: String,
    pub return_to: String,
    created_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcAuthorizationStart {
    pub state: String,
    pub nonce: String,
    pub code_verifier: String,
    pub return_to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedOidcClaims {
    pub issuer: String,
    pub subject: String,
    pub claims: Value,
}

#[derive(Default)]
pub struct PendingOidcLogins {
    states: Mutex<HashMap<String, PendingOidcLogin>>,
}

impl PendingOidcLogins {
    pub async fn start(
        &self,
        return_to: Option<&str>,
        path_prefix: &str,
    ) -> Result<OidcAuthorizationStart, OidcError> {
        self.prune_expired(Instant::now()).await;

        let state = random_urlsafe().map_err(|_| OidcError::InvalidState)?;
        let nonce = random_urlsafe().map_err(|_| OidcError::InvalidState)?;
        let code_verifier = random_urlsafe().map_err(|_| OidcError::InvalidState)?;
        let return_to = validated_return_target(return_to, path_prefix)?;

        let login = PendingOidcLogin {
            nonce: nonce.clone(),
            code_verifier: code_verifier.clone(),
            return_to: return_to.clone(),
            created_at: Instant::now(),
        };

        let mut states = self.states.lock().await;
        if states.len() >= MAX_PENDING_LOGINS
            && let Some(oldest_state) = states
                .iter()
                .min_by_key(|(_, login)| login.created_at)
                .map(|(state, _)| state.clone())
        {
            states.remove(&oldest_state);
        }
        states.insert(state.clone(), login);

        Ok(OidcAuthorizationStart {
            state,
            nonce,
            code_verifier,
            return_to,
        })
    }

    pub async fn take(&self, state: &str) -> Result<PendingOidcLogin, OidcError> {
        let Some(login) = self.states.lock().await.remove(state) else {
            return Err(OidcError::InvalidState);
        };

        if login.created_at.elapsed() > PENDING_LOGIN_TTL {
            return Err(OidcError::ExpiredState);
        }

        Ok(login)
    }

    async fn prune_expired(&self, now: Instant) {
        self.states
            .lock()
            .await
            .retain(|_, login| now.duration_since(login.created_at) <= PENDING_LOGIN_TTL);
    }

    #[cfg(test)]
    pub async fn insert_for_test(
        &self,
        state: String,
        nonce: String,
        code_verifier: String,
        return_to: String,
        created_at: Instant,
    ) {
        self.states.lock().await.insert(
            state,
            PendingOidcLogin {
                nonce,
                code_verifier,
                return_to,
                created_at,
            },
        );
    }

    #[cfg(test)]
    pub async fn len_for_test(&self) -> usize {
        self.states.lock().await.len()
    }
}

pub fn username_from_claims(claims: &Value, username_claim: &str) -> Result<String, OidcError> {
    let Some(username) = claims.get(username_claim).and_then(Value::as_str) else {
        return Err(OidcError::MissingUsernameClaim);
    };
    let username = username.trim();
    if username.is_empty() {
        return Err(OidcError::MissingUsernameClaim);
    }
    Ok(username.to_string())
}

pub fn validated_return_target(
    return_to: Option<&str>,
    path_prefix: &str,
) -> Result<String, OidcError> {
    let default_target = if path_prefix.is_empty() {
        "/".to_string()
    } else {
        path_prefix.to_string()
    };

    let Some(return_to) = return_to else {
        return Ok(default_target);
    };

    if return_to.is_empty() {
        return Ok(default_target);
    }

    if return_to.chars().any(char::is_control)
        || Url::parse(return_to).is_ok()
        || return_to.starts_with("//")
        || !return_to.starts_with('/')
        || return_to.contains('\\')
    {
        return Err(OidcError::InvalidReturnTarget);
    }

    if path_prefix.is_empty()
        || return_to == path_prefix
        || return_to
            .strip_prefix(path_prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
    {
        Ok(return_to.to_string())
    } else {
        Err(OidcError::InvalidReturnTarget)
    }
}

pub fn login_path(config: &OidcConfig, path_prefix: &str) -> String {
    let _ = config;
    format!("{path_prefix}/api/oidc/login")
}

pub async fn authorization_url(
    config: &OidcConfig,
    start: &OidcAuthorizationStart,
) -> Result<Url, OidcError> {
    let client = discover_client(config).await?;
    let code_verifier = PkceCodeVerifier::new(start.code_verifier.clone());
    let code_challenge = PkceCodeChallenge::from_code_verifier_sha256(&code_verifier);

    let state = start.state.clone();
    let nonce = start.nonce.clone();
    let mut request = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            move || CsrfToken::new(state),
            move || Nonce::new(nonce),
        )
        .set_pkce_challenge(code_challenge)
        .add_scope(Scope::new("openid".to_string()));

    for scope in config
        .scopes
        .iter()
        .filter(|scope| scope.as_str() != "openid")
    {
        request = request.add_scope(Scope::new(scope.clone()));
    }

    let (url, _, _) = request.url();
    Ok(url)
}

pub async fn exchange_code_and_validate(
    config: &OidcConfig,
    code: String,
    pending: PendingOidcLogin,
) -> Result<VerifiedOidcClaims, OidcError> {
    let http_client = oidc_http_client()?;
    let client = discover_client_with_http(config, &http_client).await?;

    let token_response = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(|_| OidcError::ProviderRequest)?
        .set_pkce_verifier(PkceCodeVerifier::new(pending.code_verifier))
        .request_async(&http_client)
        .await
        .map_err(|_| OidcError::ProviderRequest)?;

    let id_token = token_response.id_token().ok_or(OidcError::MissingIdToken)?;
    let verifier = client.id_token_verifier();
    let claims = id_token
        .claims(&verifier, &Nonce::new(pending.nonce))
        .map_err(|_| OidcError::InvalidIdToken)?;

    if let Some(expected_access_token_hash) = claims.access_token_hash() {
        let actual_access_token_hash = AccessTokenHash::from_token(
            token_response.access_token(),
            id_token
                .signing_alg()
                .map_err(|_| OidcError::InvalidIdToken)?,
            id_token
                .signing_key(&verifier)
                .map_err(|_| OidcError::InvalidIdToken)?,
        )
        .map_err(|_| OidcError::InvalidAccessTokenHash)?;

        if actual_access_token_hash != *expected_access_token_hash {
            return Err(OidcError::InvalidAccessTokenHash);
        }
    }

    let claims = serde_json::to_value(claims).map_err(|_| OidcError::InvalidTokenResponse)?;
    let issuer = claims
        .get("iss")
        .and_then(Value::as_str)
        .ok_or(OidcError::InvalidIdToken)?
        .to_string();
    let subject = claims
        .get("sub")
        .and_then(Value::as_str)
        .ok_or(OidcError::InvalidIdToken)?
        .to_string();

    Ok(VerifiedOidcClaims {
        issuer,
        subject,
        claims,
    })
}

async fn discover_client(config: &OidcConfig) -> Result<DiscoveredCoreClient, OidcError> {
    let http_client = oidc_http_client()?;
    discover_client_with_http(config, &http_client).await
}

async fn discover_client_with_http(
    config: &OidcConfig,
    http_client: &reqwest::Client,
) -> Result<DiscoveredCoreClient, OidcError> {
    let provider_metadata = CoreProviderMetadata::discover_async(
        IssuerUrl::new(config.issuer_url.clone()).map_err(|_| OidcError::ProviderRequest)?,
        http_client,
    )
    .await
    .map_err(|_| OidcError::ProviderRequest)?;

    let client_secret = config.client_secret.clone().map(ClientSecret::new);
    Ok(CoreClient::from_provider_metadata(
        provider_metadata,
        ClientId::new(config.client_id.clone()),
        client_secret,
    )
    .set_redirect_uri(
        RedirectUrl::new(config.redirect_url.clone()).map_err(|_| OidcError::ProviderRequest)?,
    ))
}

fn oidc_http_client() -> Result<reqwest::Client, OidcError> {
    reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| OidcError::ProviderRequest)
}

pub fn validate_oidc_startup_config(config: &common::config::Config) -> Result<(), anyhow::Error> {
    let Some(oidc) = &config.web_server.oidc else {
        return Ok(());
    };

    let issuer = Url::parse(&oidc.issuer_url)
        .map_err(|err| anyhow::anyhow!("invalid OIDC issuer_url: {err}"))?;
    match issuer.scheme() {
        "https" => {}
        "http" if issuer.host_str().is_some_and(is_loopback_host) => {}
        _ => {
            return Err(anyhow::anyhow!(
                "invalid OIDC issuer_url: issuer must use HTTPS unless it is a loopback development issuer"
            ));
        }
    }

    let redirect = Url::parse(&oidc.redirect_url)
        .map_err(|err| anyhow::anyhow!("invalid OIDC redirect_url: {err}"))?;
    if redirect.scheme() != "https" {
        return Err(anyhow::anyhow!(
            "invalid OIDC redirect_url: redirect URL must use HTTPS"
        ));
    }

    let expected_path = format!("{}/api/oidc/callback", config.web_server.url_path_prefix);
    if redirect.path() != expected_path {
        return Err(anyhow::anyhow!(
            "invalid OIDC redirect_url: path must be {expected_path:?}"
        ));
    }

    if !config.web_server.session_cookie_secure {
        return Err(anyhow::anyhow!(
            "invalid OIDC configuration: session_cookie_secure must be true when OIDC is enabled"
        ));
    }

    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|addr| addr.is_loopback())
}

fn random_urlsafe() -> Result<String, openssl::error::ErrorStack> {
    let mut bytes = [0; OIDC_RANDOM_BYTES];
    rand_bytes(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        net::TcpListener,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use actix_web::{App as ActixApp, HttpResponse, HttpServer, web};
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use common::config::OidcConfig;
    use openssl::{
        hash::MessageDigest,
        pkey::{PKey, Private},
        rsa::Rsa,
        sign::Signer,
    };
    use serde_json::json;
    use sha2::{Digest, Sha256};

    use super::{
        MAX_PENDING_LOGINS, OidcError, PendingOidcLogin, PendingOidcLogins, authorization_url,
        exchange_code_and_validate, username_from_claims, validate_oidc_startup_config,
        validated_return_target,
    };

    fn rs256_id_token(
        issuer: &str,
        client_id: &str,
        signing_key: &PKey<Private>,
        nonce: &str,
        subject: &str,
        access_token: &str,
        expires_in: i64,
    ) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_secs();
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","kid":"test-key","typ":"JWT"}"#);
        let access_token_hash = Sha256::digest(access_token.as_bytes());
        let at_hash = URL_SAFE_NO_PAD.encode(&access_token_hash[..16]);
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({
                "iss": issuer,
                "sub": subject,
                "aud": client_id,
                "exp": (now as i64 + expires_in) as u64,
                "iat": now,
                "nonce": nonce,
                "at_hash": at_hash
            }))
            .expect("claims should serialize"),
        );
        let signing_input = format!("{header}.{payload}");
        let mut signer =
            Signer::new(MessageDigest::sha256(), signing_key).expect("RSA signer should build");
        signer
            .update(signing_input.as_bytes())
            .expect("RSA input should be accepted");
        let signature = signer.sign_to_vec().expect("RSA key should sign");
        format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(signature))
    }

    #[actix_web::test]
    async fn authorization_url_uses_discovery_state_nonce_and_pkce() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let issuer = format!(
            "http://{}",
            listener.local_addr().expect("listener should have address")
        );
        let provider_issuer = issuer.clone();
        let server = HttpServer::new(move || {
            let metadata_issuer = provider_issuer.clone();
            ActixApp::new()
                .route(
                    "/.well-known/openid-configuration",
                    web::get().to(move || {
                        let issuer = metadata_issuer.clone();
                        async move {
                            HttpResponse::Ok().json(json!({
                                "issuer": issuer,
                                "authorization_endpoint": format!("{issuer}/authorize"),
                                "token_endpoint": format!("{issuer}/token"),
                                "jwks_uri": format!("{issuer}/jwks"),
                                "response_types_supported": ["code"],
                                "subject_types_supported": ["public"],
                                "id_token_signing_alg_values_supported": ["RS256"],
                                "token_endpoint_auth_methods_supported": ["none"]
                            }))
                        }
                    }),
                )
                .route(
                    "/jwks",
                    web::get().to(|| async { HttpResponse::Ok().json(json!({"keys": []})) }),
                )
        })
        .listen(listener)
        .expect("test provider should listen")
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);

        let config = OidcConfig {
            issuer_url: issuer.clone(),
            client_id: "moonlight-web".to_string(),
            client_secret: None,
            redirect_url: "https://stream.example/api/oidc/callback".to_string(),
            scopes: vec!["profile".to_string()],
            username_claim: "preferred_username".to_string(),
            auto_create_missing_user: false,
            display_label: "SSO".to_string(),
        };
        let pending = PendingOidcLogins::default();
        let start = pending
            .start(Some("/admin.html"), "")
            .await
            .expect("pending login should start");

        let url = authorization_url(&config, &start)
            .await
            .expect("authorization URL should be built");
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();

        let expected_authorize_endpoint = format!("{issuer}/authorize");
        assert_eq!(
            url.as_str().split('?').next(),
            Some(expected_authorize_endpoint.as_str())
        );
        assert_eq!(query.get("client_id"), Some(&"moonlight-web".to_string()));
        assert_eq!(query.get("response_type"), Some(&"code".to_string()));
        assert_eq!(query.get("redirect_uri"), Some(&config.redirect_url));
        assert_eq!(query.get("state"), Some(&start.state));
        assert_eq!(query.get("nonce"), Some(&start.nonce));
        assert_eq!(
            query.get("code_challenge_method"),
            Some(&"S256".to_string())
        );
        assert!(
            query
                .get("code_challenge")
                .is_some_and(|value| !value.is_empty())
        );
        let scope = query.get("scope").expect("scope should be present");
        assert!(scope.split_whitespace().any(|scope| scope == "openid"));
        assert!(scope.split_whitespace().any(|scope| scope == "profile"));

        handle.stop(true).await;
    }

    #[actix_web::test]
    async fn token_exchange_cryptographically_validates_id_token_and_nonce() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let issuer = format!(
            "http://{}",
            listener.local_addr().expect("listener should have address")
        );
        let client_id = "moonlight-web".to_string();
        let rsa = Rsa::generate(2048).expect("test RSA key should generate");
        let jwk = json!({
            "kty": "RSA",
            "use": "sig",
            "kid": "test-key",
            "alg": "RS256",
            "n": URL_SAFE_NO_PAD.encode(rsa.n().to_vec()),
            "e": URL_SAFE_NO_PAD.encode(rsa.e().to_vec())
        });
        let signing_key = PKey::from_rsa(rsa).expect("test RSA key should import");
        let provider_issuer = issuer.clone();
        let provider_client_id = client_id.clone();
        let provider_jwk = jwk.clone();
        let provider_signing_key = signing_key.clone();
        let server = HttpServer::new(move || {
            let metadata_issuer = provider_issuer.clone();
            let token_issuer = provider_issuer.clone();
            let token_client_id = provider_client_id.clone();
            let jwk = provider_jwk.clone();
            let token_signing_key = provider_signing_key.clone();
            ActixApp::new()
                .route(
                    "/.well-known/openid-configuration",
                    web::get().to(move || {
                        let issuer = metadata_issuer.clone();
                        async move {
                            HttpResponse::Ok().json(json!({
                                "issuer": issuer,
                                "authorization_endpoint": format!("{issuer}/authorize"),
                                "token_endpoint": format!("{issuer}/token"),
                                "jwks_uri": format!("{issuer}/jwks"),
                                "response_types_supported": ["code"],
                                "subject_types_supported": ["public"],
                                "id_token_signing_alg_values_supported": ["RS256"],
                                "token_endpoint_auth_methods_supported": ["none"]
                            }))
                        }
                    }),
                )
                .route(
                    "/jwks",
                    web::get().to(move || {
                        let jwk = jwk.clone();
                        async move { HttpResponse::Ok().json(json!({"keys": [jwk]})) }
                    }),
                )
                .route(
                    "/token",
                    web::post().to(move |form: web::Form<HashMap<String, String>>| {
                        let code = form.get("code").map(String::as_str).unwrap_or_default();
                        let (issuer, client_id, nonce, access_token, expires_in) = match code {
                            "wrong-issuer" => (
                                format!("{token_issuer}/other"),
                                token_client_id.clone(),
                                "expected-nonce",
                                "access-token",
                                300,
                            ),
                            "wrong-audience" => (
                                token_issuer.clone(),
                                "other-client".to_string(),
                                "expected-nonce",
                                "access-token",
                                300,
                            ),
                            "wrong-at-hash" => (
                                token_issuer.clone(),
                                token_client_id.clone(),
                                "expected-nonce",
                                "different-access-token",
                                300,
                            ),
                            "expired" => (
                                token_issuer.clone(),
                                token_client_id.clone(),
                                "expected-nonce",
                                "access-token",
                                -300,
                            ),
                            _ => (
                                token_issuer.clone(),
                                token_client_id.clone(),
                                "expected-nonce",
                                "access-token",
                                300,
                            ),
                        };
                        let id_token = rs256_id_token(
                            &issuer,
                            &client_id,
                            &token_signing_key,
                            nonce,
                            "alice-subject",
                            access_token,
                            expires_in,
                        );
                        async move {
                            HttpResponse::Ok().json(json!({
                                "access_token": "access-token",
                                "token_type": "Bearer",
                                "expires_in": 300,
                                "id_token": id_token
                            }))
                        }
                    }),
                )
        })
        .listen(listener)
        .expect("test provider should listen")
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);

        let config = OidcConfig {
            issuer_url: issuer,
            client_id,
            client_secret: None,
            redirect_url: "https://stream.example/api/oidc/callback".to_string(),
            scopes: vec!["openid".to_string()],
            username_claim: "sub".to_string(),
            auto_create_missing_user: false,
            display_label: "SSO".to_string(),
        };
        let valid_pending = PendingOidcLogin {
            nonce: "expected-nonce".to_string(),
            code_verifier: "0123456789abcdef0123456789abcdef0123456789abc".to_string(),
            return_to: "/".to_string(),
            created_at: Instant::now(),
        };
        let claims = exchange_code_and_validate(&config, "valid-code".to_string(), valid_pending)
            .await
            .expect("valid signed ID token should pass");
        assert_eq!(claims.issuer, config.issuer_url);
        assert_eq!(claims.subject, "alice-subject");
        assert_eq!(
            claims.claims.get("sub").and_then(|value| value.as_str()),
            Some("alice-subject")
        );

        let invalid_nonce = PendingOidcLogin {
            nonce: "different-nonce".to_string(),
            code_verifier: "0123456789abcdef0123456789abcdef0123456789abc".to_string(),
            return_to: "/".to_string(),
            created_at: Instant::now(),
        };
        assert!(matches!(
            exchange_code_and_validate(&config, "valid-code".to_string(), invalid_nonce).await,
            Err(OidcError::InvalidIdToken)
        ));

        for (code, expected_error) in [
            ("wrong-at-hash", OidcError::InvalidAccessTokenHash),
            ("wrong-issuer", OidcError::InvalidIdToken),
            ("wrong-audience", OidcError::InvalidIdToken),
            ("expired", OidcError::InvalidIdToken),
        ] {
            let pending = PendingOidcLogin {
                nonce: "expected-nonce".to_string(),
                code_verifier: "0123456789abcdef0123456789abcdef0123456789abc".to_string(),
                return_to: "/".to_string(),
                created_at: Instant::now(),
            };
            assert_eq!(
                exchange_code_and_validate(&config, code.to_string(), pending).await,
                Err(expected_error)
            );
        }

        handle.stop(true).await;
    }

    #[tokio::test]
    async fn pending_oidc_state_is_one_time() {
        let pending = PendingOidcLogins::default();
        let start = pending
            .start(Some("/app"), "")
            .await
            .expect("state should be created");

        let login = pending
            .take(&start.state)
            .await
            .expect("first use should succeed");
        assert_eq!(login.nonce, start.nonce);
        assert_eq!(login.code_verifier, start.code_verifier);
        assert_eq!(login.return_to, "/app");

        let replay = pending.take(&start.state).await;
        assert!(matches!(replay, Err(OidcError::InvalidState)));
    }

    #[tokio::test]
    async fn pending_oidc_state_evicts_oldest_when_full() {
        let pending = PendingOidcLogins::default();
        let oldest = Instant::now() - Duration::from_secs(60);
        pending
            .insert_for_test(
                "oldest".to_string(),
                "nonce".to_string(),
                "verifier".to_string(),
                "/".to_string(),
                oldest,
            )
            .await;
        for index in 1..MAX_PENDING_LOGINS {
            pending
                .insert_for_test(
                    format!("state-{index}"),
                    "nonce".to_string(),
                    "verifier".to_string(),
                    "/".to_string(),
                    Instant::now(),
                )
                .await;
        }

        pending
            .start(Some("/app"), "")
            .await
            .expect("new state should evict oldest instead of failing");

        assert_eq!(pending.len_for_test().await, MAX_PENDING_LOGINS);
        assert!(matches!(
            pending.take("oldest").await,
            Err(OidcError::InvalidState)
        ));
    }

    #[tokio::test]
    async fn pending_oidc_state_expires() {
        let pending = PendingOidcLogins::default();
        pending
            .insert_for_test(
                "state".to_string(),
                "nonce".to_string(),
                "verifier".to_string(),
                "/".to_string(),
                Instant::now() - Duration::from_secs(301),
            )
            .await;

        let result = pending.take("state").await;
        assert!(matches!(result, Err(OidcError::ExpiredState)));
    }

    #[test]
    fn username_claim_must_exist_and_be_non_empty_string() {
        assert_eq!(
            username_from_claims(
                &json!({"preferred_username": "alice"}),
                "preferred_username"
            ),
            Ok("alice".to_string())
        );
        assert_eq!(
            username_from_claims(&json!({"preferred_username": ""}), "preferred_username"),
            Err(OidcError::MissingUsernameClaim)
        );
        assert_eq!(
            username_from_claims(&json!({"preferred_username": 123}), "preferred_username"),
            Err(OidcError::MissingUsernameClaim)
        );
        assert_eq!(
            username_from_claims(&json!({"sub": "alice"}), "preferred_username"),
            Err(OidcError::MissingUsernameClaim)
        );
    }

    #[test]
    fn return_target_rejects_open_redirects() {
        assert_eq!(
            validated_return_target(None, "/moonlight"),
            Ok("/moonlight".to_string())
        );
        assert_eq!(
            validated_return_target(Some("/moonlight/admin.html"), "/moonlight"),
            Ok("/moonlight/admin.html".to_string())
        );
        assert_eq!(
            validated_return_target(Some("/moonlight-evil"), "/moonlight"),
            Err(OidcError::InvalidReturnTarget)
        );
        assert_eq!(
            validated_return_target(Some("https://evil.example"), "/moonlight"),
            Err(OidcError::InvalidReturnTarget)
        );
        assert_eq!(
            validated_return_target(Some("//evil.example"), "/moonlight"),
            Err(OidcError::InvalidReturnTarget)
        );
        assert_eq!(
            validated_return_target(Some("/other"), "/moonlight"),
            Err(OidcError::InvalidReturnTarget)
        );
        assert_eq!(
            validated_return_target(Some("/moonlight/admin.html\n"), "/moonlight"),
            Err(OidcError::InvalidReturnTarget)
        );
    }

    #[test]
    fn oidc_startup_config_requires_secure_urls_matching_callback_path() {
        let mut config = common::config::Config::default();
        config.web_server.session_cookie_secure = true;
        config.web_server.oidc = Some(OidcConfig {
            issuer_url: "https://idp.example.com/realms/moonlight".to_string(),
            client_id: "moonlight-web".to_string(),
            client_secret: None,
            redirect_url: "https://example.com/api/oidc/callback".to_string(),
            scopes: vec!["openid".to_string()],
            username_claim: "preferred_username".to_string(),
            auto_create_missing_user: false,
            display_label: "SSO".to_string(),
        });

        validate_oidc_startup_config(&config).expect("secure config should validate");

        config
            .web_server
            .oidc
            .as_mut()
            .expect("oidc config should be set")
            .redirect_url = "http://example.com/api/oidc/callback".to_string();
        assert!(validate_oidc_startup_config(&config).is_err());

        config
            .web_server
            .oidc
            .as_mut()
            .expect("oidc config should be set")
            .redirect_url = "https://example.com/wrong/api/oidc/callback".to_string();
        assert!(validate_oidc_startup_config(&config).is_err());

        config
            .web_server
            .oidc
            .as_mut()
            .expect("oidc config should be set")
            .redirect_url = "https://example.com/api/oidc/callback".to_string();
        config.web_server.session_cookie_secure = false;
        assert!(validate_oidc_startup_config(&config).is_err());

        config.web_server.session_cookie_secure = true;
        config
            .web_server
            .oidc
            .as_mut()
            .expect("oidc config should be set")
            .issuer_url = "http://idp.example.com/realms/moonlight".to_string();
        assert!(validate_oidc_startup_config(&config).is_err());

        config
            .web_server
            .oidc
            .as_mut()
            .expect("oidc config should be set")
            .issuer_url = "http://127.0.0.1:8080/realms/moonlight".to_string();
        validate_oidc_startup_config(&config).expect("loopback HTTP issuer should be allowed");
    }
}
