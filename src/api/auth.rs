use actix_web::{
    Error, FromRequest, HttpRequest, HttpResponse,
    body::MessageBody,
    cookie::{Cookie, Expiration, SameSite, time::OffsetDateTime},
    dev::{Payload, ServiceRequest, ServiceResponse},
    get,
    middleware::Next,
    post,
    web::{Data, Json, Query},
};
use common::api_bindings::{AuthMetadataResponse, OidcAuthMetadata, PostLoginRequest};
use futures::future::{Ready, ready};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{pin::Pin, time::Duration};
use tracing::warn;

use crate::app::{
    App, AppError,
    auth::{SessionToken, UserAuth},
    oidc::{
        OidcError, authorization_url, exchange_code_and_validate, login_path, username_from_claims,
    },
    user::{Admin, AuthenticatedUser},
};

pub const COOKIE_SESSION_TOKEN_NAME: &str = "mlSession";
const COOKIE_OIDC_STATE_NAME: &str = "mlOidcState";
const OIDC_STATE_COOKIE_TTL: Duration = Duration::from_secs(5 * 60);

impl FromRequest for UserAuth {
    type Error = AppError;

    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        ready(extract_user_auth(req))
    }
}
fn extract_user_auth(req: &HttpRequest) -> Result<UserAuth, AppError> {
    let app = match req.app_data::<Data<App>>() {
        None => return Err(AppError::AppDestroyed),
        Some(value) => value,
    };

    if let Some(header_auth) = &app.config().web_server.forwarded_header
        && let Some(username) = req.headers().get(&header_auth.username_header)
    {
        let Ok(username) = username.to_str() else {
            return Err(AppError::HeaderAuthMalformed);
        };

        Ok(UserAuth::ForwardedHeaders {
            username: username.to_string(),
        })
    } else if let Some(bearer) = req.headers().get("Authorization") {
        // Look for bearer
        let Ok(bearer) = bearer.to_str() else {
            return Err(AppError::BearerMalformed);
        };

        let token_str = bearer
            .strip_prefix("Bearer")
            .ok_or(AppError::AuthorizationNotBearer)?
            .trim();

        let token = SessionToken::decode(token_str)?;

        Ok(UserAuth::Session(token))
    } else if let Some(cookie) = req.cookie(COOKIE_SESSION_TOKEN_NAME) {
        // Look for cookie
        let token = SessionToken::decode(cookie.value())?;

        Ok(UserAuth::Session(token))
    } else {
        Ok(UserAuth::None)
    }
}

impl FromRequest for AuthenticatedUser {
    type Error = AppError;

    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let app = match req.app_data::<Data<App>>() {
            None => return Box::pin(ready(Err(AppError::AppDestroyed))),
            Some(value) => value,
        };

        let auth_future = UserAuth::from_request(req, payload);

        let app = app.clone();
        Box::pin(async move {
            let auth = auth_future.await?;

            let user = app.user_by_auth(auth).await?;

            Ok(user)
        })
    }
}

impl FromRequest for Admin {
    type Error = AppError;

    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let future = AuthenticatedUser::from_request(req, payload);

        Box::pin(async move {
            let user = future.await?;

            user.into_admin().await
        })
    }
}

#[post("/login")]
async fn login(
    app: Data<App>,
    Json(request): Json<PostLoginRequest>,
) -> Result<HttpResponse, Error> {
    let user = if app.config().web_server.first_login_create_admin {
        match app
            .try_add_first_login(request.name.clone(), request.password.clone())
            .await
        {
            Ok(user) => user,
            Err(AppError::FirstUserAlreadyExists) => {
                app.user_by_auth(UserAuth::UserPassword {
                    username: request.name,
                    password: request.password,
                })
                .await?
            }
            Err(err) => return Err(err.into()),
        }
    } else {
        app.user_by_auth(UserAuth::UserPassword {
            username: request.name,
            password: request.password,
        })
        .await?
    };

    let session_expiration = app.config().web_server.session_cookie_expiration;

    let session = user.new_session(session_expiration).await?;
    let mut session_bytes = [0; _];
    let session_str = session.encode(&mut session_bytes);

    Ok(HttpResponse::Ok()
        .cookie(build_cookie(&app, session_expiration, session_str))
        .finish())
}

#[post("/logout")]
async fn logout(app: Data<App>, auth: UserAuth, req: HttpRequest) -> Result<HttpResponse, Error> {
    let session = match auth {
        UserAuth::Session(session) => session,
        _ => return Ok(HttpResponse::BadRequest().finish()),
    };

    app.delete_session(session).await?;

    let mut response = HttpResponse::Ok().finish();

    if req.cookie(COOKIE_SESSION_TOKEN_NAME).is_some() {
        response.add_removal_cookie(&build_cookie(&app, Duration::ZERO, ""))?;
    }

    Ok(response)
}

#[get("/auth/metadata")]
async fn auth_metadata(app: Data<App>) -> HttpResponse {
    let oidc = app
        .config()
        .web_server
        .oidc
        .as_ref()
        .map(|config| OidcAuthMetadata {
            display_label: config.display_label.clone(),
            login_url: login_path(config, &app.config().web_server.url_path_prefix),
        });

    HttpResponse::Ok()
        .append_header(("Cache-Control", "no-store"))
        .json(AuthMetadataResponse { oidc })
}

#[derive(Debug, Deserialize)]
struct OidcLoginQuery {
    return_to: Option<String>,
}

#[get("/oidc/login")]
async fn oidc_login(app: Data<App>, query: Query<OidcLoginQuery>) -> Result<HttpResponse, Error> {
    let Some(config) = &app.config().web_server.oidc else {
        return Ok(HttpResponse::NotFound()
            .append_header(("Cache-Control", "no-store"))
            .finish());
    };

    let start = app
        .oidc_pending_logins()
        .start(
            query.return_to.as_deref(),
            &app.config().web_server.url_path_prefix,
        )
        .await
        .map_err(|err| match err {
            OidcError::InvalidReturnTarget => {
                warn!("OIDC login failed: invalid return target");
                AppError::BadRequest
            }
            err => {
                warn!("OIDC login failed: {err}");
                AppError::BadRequest
            }
        })?;

    let authorization_url = authorization_url(config, &start).await.map_err(|err| {
        warn!("OIDC login failed: {err}");
        AppError::BadRequest
    })?;

    Ok(HttpResponse::Found()
        .cookie(build_oidc_state_cookie(&app, &start.state))
        .append_header(("Cache-Control", "no-store"))
        .append_header(("Location", authorization_url.to_string()))
        .finish())
}

#[derive(Debug, Deserialize)]
struct OidcCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[get("/oidc/callback")]
async fn oidc_callback(
    app: Data<App>,
    req: HttpRequest,
    query: Query<OidcCallbackQuery>,
) -> Result<HttpResponse, Error> {
    let Some(config) = &app.config().web_server.oidc else {
        let mut response = HttpResponse::NotFound()
            .append_header(("Cache-Control", "no-store"))
            .finish();
        response.add_removal_cookie(&build_oidc_state_cookie(&app, ""))?;
        return Ok(response);
    };

    let Some(state) = query.state.as_deref() else {
        warn!("OIDC callback failed: missing state");
        return Ok(generic_oidc_bad_request(&app));
    };
    if !oidc_state_matches(&req, state) {
        warn!("OIDC callback failed: state cookie mismatch");
        return Ok(generic_oidc_bad_request(&app));
    }

    let pending = match app.oidc_pending_logins().take(state).await {
        Ok(pending) => pending,
        Err(_) => {
            warn!("OIDC callback failed: pending state invalid or expired");
            return Ok(generic_oidc_bad_request(&app));
        }
    };

    if query.error.is_some() {
        warn!("OIDC callback failed: provider returned an error");
        return Ok(generic_oidc_bad_request(&app));
    }
    let _ = &query.error_description;

    let Some(code) = query.code.clone() else {
        warn!("OIDC callback failed: missing authorization code");
        return Ok(generic_oidc_bad_request(&app));
    };

    let verified = match exchange_code_and_validate(config, code, pending.clone()).await {
        Ok(claims) => claims,
        Err(err) => {
            warn!("OIDC callback failed: {err}");
            return Ok(generic_oidc_bad_request(&app));
        }
    };
    let username = match username_from_claims(&verified.claims, &config.username_claim) {
        Ok(username) => username,
        Err(err) => {
            warn!("OIDC callback failed: {err}");
            return Ok(generic_oidc_bad_request(&app));
        }
    };
    let user = match app
        .user_by_oidc_identity(verified.issuer, verified.subject, username)
        .await
    {
        Ok(user) => user,
        Err(err) => {
            warn!("OIDC callback failed: {err}");
            return Ok(generic_oidc_bad_request(&app));
        }
    };

    let session_expiration = app.config().web_server.session_cookie_expiration;
    let session = match user.new_session(session_expiration).await {
        Ok(session) => session,
        Err(err) => {
            warn!("OIDC callback failed: session creation failed");
            let _ = err;
            return Ok(generic_oidc_bad_request(&app));
        }
    };
    let mut session_bytes = [0; _];
    let session_str = session.encode(&mut session_bytes);

    let mut response = HttpResponse::Found()
        .cookie(build_cookie(&app, session_expiration, session_str))
        .append_header(("Cache-Control", "no-store"))
        .append_header(("Location", pending.return_to))
        .finish();
    response.add_removal_cookie(&build_oidc_state_cookie(&app, ""))?;
    Ok(response)
}

fn generic_oidc_bad_request(app: &App) -> HttpResponse {
    let mut response = HttpResponse::BadRequest()
        .append_header(("Cache-Control", "no-store"))
        .body("OpenID Connect login failed");
    let _ = response.add_removal_cookie(&build_oidc_state_cookie(app, ""));
    response
}

fn oidc_state_matches(req: &HttpRequest, expected_state: &str) -> bool {
    let Some(cookie) = req.cookie(COOKIE_OIDC_STATE_NAME) else {
        return false;
    };

    Sha256::digest(cookie.value().as_bytes()) == Sha256::digest(expected_state.as_bytes())
}

fn build_oidc_state_cookie<'a>(app: &'a App, state: &'a str) -> Cookie<'a> {
    Cookie::build(COOKIE_OIDC_STATE_NAME, state)
        .path(format!(
            "{}/api/oidc/callback",
            app.config().web_server.url_path_prefix
        ))
        .same_site(SameSite::Lax)
        .http_only(true)
        .secure(app.config().web_server.session_cookie_secure)
        .expires(Expiration::DateTime(
            OffsetDateTime::now_utc() + OIDC_STATE_COOKIE_TTL,
        ))
        .finish()
}

pub async fn auth_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let Some(app) = req.app_data::<Data<App>>().cloned() else {
        return Err(AppError::AppDestroyed.into());
    };

    let mut response = next.call(req).await?;
    if let Some(err) = response.response().error()
        && let Some(AppError::SessionTokenNotFound) = err.as_error::<AppError>()
    {
        response
            .response_mut()
            .add_removal_cookie(&build_cookie(&app, Duration::ZERO, ""))?;
    }

    Ok(response)
}

pub fn build_cookie<'a>(app: &'a App, expiration: Duration, session_str: &'a str) -> Cookie<'a> {
    let path = if app.config().web_server.url_path_prefix.is_empty() {
        "/"
    } else {
        &app.config().web_server.url_path_prefix
    };
    Cookie::build(COOKIE_SESSION_TOKEN_NAME, session_str)
        .path(path)
        .same_site(SameSite::Strict)
        .http_only(true) // not accessible via js
        .secure(app.config().web_server.session_cookie_secure)
        .expires(Expiration::DateTime(OffsetDateTime::now_utc() + expiration))
        .finish()
}

#[get("/authenticate")]
async fn authenticate(_user: AuthenticatedUser) -> HttpResponse {
    HttpResponse::Ok().finish()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use actix_web::{App as ActixApp, cookie::Cookie, http::StatusCode, test, web::Data};
    use common::{
        api_bindings::AuthMetadataResponse,
        config::{Config, OidcConfig, StorageConfig},
    };

    use crate::{api::api_service, app::App};

    use super::{COOKIE_OIDC_STATE_NAME, build_cookie};

    fn has_oidc_state_removal_cookie(response: &actix_web::dev::ServiceResponse) -> bool {
        response
            .headers()
            .get_all("Set-Cookie")
            .filter_map(|value| value.to_str().ok())
            .any(|value| {
                value.starts_with(&format!("{COOKIE_OIDC_STATE_NAME}="))
                    && value.contains("Max-Age=0")
            })
    }

    fn test_config() -> Config {
        let mut config = Config::default();
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        config.data_storage = StorageConfig::Json {
            path: std::env::temp_dir()
                .join(format!("moonlight-web-stream-oidc-auth-test-{suffix}.json"))
                .display()
                .to_string(),
            session_expiration_check_interval: Duration::from_secs(60),
        };
        config.web_server.session_cookie_secure = true;
        config
    }

    fn test_oidc_config() -> OidcConfig {
        OidcConfig {
            issuer_url: "https://idp.example.com/realms/moonlight".to_string(),
            client_id: "moonlight-web".to_string(),
            client_secret: None,
            redirect_url: "https://example.com/api/oidc/callback".to_string(),
            scopes: vec!["openid".to_string(), "profile".to_string()],
            username_claim: "preferred_username".to_string(),
            auto_create_missing_user: false,
            display_label: "Company SSO".to_string(),
        }
    }

    #[actix_web::test]
    async fn session_cookie_path_defaults_to_root_and_honors_prefix() {
        let app = App::new(test_config()).await.expect("app should start");
        assert_eq!(
            build_cookie(&app, Duration::from_secs(60), "session").path(),
            Some("/")
        );

        let mut config = test_config();
        config.web_server.url_path_prefix = "/moonlight".to_string();
        let app = App::new(config).await.expect("app should start");
        assert_eq!(
            build_cookie(&app, Duration::from_secs(60), "session").path(),
            Some("/moonlight")
        );
    }

    #[actix_web::test]
    async fn auth_metadata_hides_oidc_when_unconfigured() {
        let app = App::new(test_config()).await.expect("app should start");
        let service = test::init_service(
            ActixApp::new()
                .app_data(Data::new(app))
                .service(api_service()),
        )
        .await;

        let request = test::TestRequest::get()
            .uri("/api/auth/metadata")
            .to_request();
        let response: AuthMetadataResponse = test::call_and_read_body_json(&service, request).await;

        assert_eq!(response.oidc, None);
    }

    #[actix_web::test]
    async fn auth_metadata_returns_oidc_label_and_login_url_when_configured() {
        let mut config = test_config();
        config.web_server.url_path_prefix = "/moonlight".to_string();
        let mut oidc = test_oidc_config();
        oidc.redirect_url = "https://example.com/moonlight/api/oidc/callback".to_string();
        config.web_server.oidc = Some(oidc);
        let app = App::new(config).await.expect("app should start");
        let service = test::init_service(
            ActixApp::new()
                .app_data(Data::new(app))
                .service(api_service()),
        )
        .await;

        let request = test::TestRequest::get()
            .uri("/api/auth/metadata")
            .to_request();
        let response = test::call_service(&service, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("Cache-Control")
                .expect("cache-control header should be set"),
            "no-store"
        );
        let response: AuthMetadataResponse = test::read_body_json(response).await;

        let oidc = response.oidc.expect("oidc metadata should be present");
        assert_eq!(oidc.display_label, "Company SSO");
        assert_eq!(oidc.login_url, "/moonlight/api/oidc/login");
    }

    #[actix_web::test]
    async fn oidc_login_rejects_invalid_return_target_before_discovery() {
        let mut config = test_config();
        config.web_server.url_path_prefix = "/moonlight".to_string();
        let mut oidc = test_oidc_config();
        oidc.redirect_url = "https://example.com/moonlight/api/oidc/callback".to_string();
        config.web_server.oidc = Some(oidc);
        let app = App::new(config).await.expect("app should start");
        let service = test::init_service(
            ActixApp::new()
                .app_data(Data::new(app))
                .service(api_service()),
        )
        .await;

        let request = test::TestRequest::get()
            .uri("/api/oidc/login?return_to=%2Fmoonlight-evil")
            .to_request();
        let response = test::call_service(&service, request).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn oidc_login_rejects_control_characters_in_return_target_before_discovery() {
        let mut config = test_config();
        config.web_server.oidc = Some(test_oidc_config());
        let app = App::new(config).await.expect("app should start");
        let service = test::init_service(
            ActixApp::new()
                .app_data(Data::new(app))
                .service(api_service()),
        )
        .await;

        let request = test::TestRequest::get()
            .uri("/api/oidc/login?return_to=%2Fadmin.html%0A")
            .to_request();
        let response = test::call_service(&service, request).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn oidc_callback_rejects_provider_error_without_state() {
        let mut config = test_config();
        config.web_server.oidc = Some(test_oidc_config());
        let app = App::new(config).await.expect("app should start");
        let service = test::init_service(
            ActixApp::new()
                .app_data(Data::new(app))
                .service(api_service()),
        )
        .await;

        let request = test::TestRequest::get()
            .uri("/api/oidc/callback?error=access_denied&error_description=secret")
            .to_request();
        let response = test::call_service(&service, request).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get("Cache-Control")
                .expect("cache-control header should be set"),
            "no-store"
        );
    }

    #[actix_web::test]
    async fn oidc_callback_removes_state_cookie_when_oidc_is_unconfigured() {
        let app = App::new(test_config()).await.expect("app should start");
        let service = test::init_service(
            ActixApp::new()
                .app_data(Data::new(app))
                .service(api_service()),
        )
        .await;

        let request = test::TestRequest::get()
            .uri("/api/oidc/callback?state=state&code=code")
            .cookie(Cookie::new(COOKIE_OIDC_STATE_NAME, "state"))
            .to_request();
        let response = test::call_service(&service, request).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(has_oidc_state_removal_cookie(&response));
    }

    #[actix_web::test]
    async fn oidc_callback_rejects_missing_code_and_state() {
        let mut config = test_config();
        config.web_server.oidc = Some(test_oidc_config());
        let app = App::new(config).await.expect("app should start");
        let service = test::init_service(
            ActixApp::new()
                .app_data(Data::new(app))
                .service(api_service()),
        )
        .await;

        let response = test::call_service(
            &service,
            test::TestRequest::get()
                .uri("/api/oidc/callback")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let response = test::call_service(
            &service,
            test::TestRequest::get()
                .uri("/api/oidc/callback?state=state")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn oidc_callback_rejects_invalid_expired_and_replayed_state() {
        let mut config = test_config();
        config.web_server.oidc = Some(test_oidc_config());
        let app = App::new(config).await.expect("app should start");

        app.oidc_pending_logins()
            .insert_for_test(
                "expired".to_string(),
                "nonce".to_string(),
                "verifier".to_string(),
                "/".to_string(),
                Instant::now() - Duration::from_secs(301),
            )
            .await;
        app.oidc_pending_logins()
            .insert_for_test(
                "replayed".to_string(),
                "nonce".to_string(),
                "verifier".to_string(),
                "/".to_string(),
                Instant::now(),
            )
            .await;
        app.oidc_pending_logins()
            .take("replayed")
            .await
            .expect("test state should be consumed");

        let service = test::init_service(
            ActixApp::new()
                .app_data(Data::new(app))
                .service(api_service()),
        )
        .await;

        for state in ["missing", "expired", "replayed"] {
            let request = test::TestRequest::get()
                .uri(&format!("/api/oidc/callback?state={state}&code=code"))
                .cookie(Cookie::new(COOKIE_OIDC_STATE_NAME, state))
                .to_request();
            let response = test::call_service(&service, request).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert_eq!(
                response
                    .headers()
                    .get("Cache-Control")
                    .expect("cache-control header should be set"),
                "no-store"
            );
            assert!(has_oidc_state_removal_cookie(&response));
        }
    }

    #[actix_web::test]
    async fn oidc_callback_rejects_state_not_bound_to_browser_cookie() {
        let mut config = test_config();
        config.web_server.oidc = Some(test_oidc_config());
        let app = App::new(config).await.expect("app should start");
        app.oidc_pending_logins()
            .insert_for_test(
                "expected".to_string(),
                "nonce".to_string(),
                "verifier".to_string(),
                "/".to_string(),
                Instant::now(),
            )
            .await;
        let service = test::init_service(
            ActixApp::new()
                .app_data(Data::new(app))
                .service(api_service()),
        )
        .await;

        for cookie_state in [None, Some("attacker-state")] {
            let mut request =
                test::TestRequest::get().uri("/api/oidc/callback?state=expected&code=code");
            if let Some(cookie_state) = cookie_state {
                request = request.cookie(Cookie::new(COOKIE_OIDC_STATE_NAME, cookie_state));
            }
            let response = test::call_service(&service, request.to_request()).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
    }
}
