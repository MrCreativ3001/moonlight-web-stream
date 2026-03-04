use actix_web::{
    Error, FromRequest, HttpRequest, HttpResponse,
    body::MessageBody,
    cookie::{Cookie, Expiration, SameSite, time::OffsetDateTime},
    dev::{Payload, ServiceRequest, ServiceResponse},
    get,
    middleware::Next,
    post,
    web::{Data, Json},
};
use common::api_bindings::PostLoginRequest;
use futures::future::{Ready, ready};
use std::{pin::Pin, time::Duration};

use crate::app::{
    App, AppError,
    auth::{SessionToken, UserAuth},
    user::{Admin, AuthenticatedUser},
};

pub const COOKIE_SESSION_TOKEN_NAME: &str = "mlSession";

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
    Cookie::build(COOKIE_SESSION_TOKEN_NAME, session_str)
        .path(&app.config().web_server.url_path_prefix)
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
