use bytes::Bytes;
use pem::Pem;
use reqwest::{Certificate, Client, ClientBuilder, Identity};
use thiserror::Error;
use tracing::{Level, event, instrument};
use url::{ParseError, Url};

use crate::network::{
    ApiError,
    backend::{DEFAULT_LONG_TIMEOUT, DEFAULT_TIMEOUT},
    request_client::{QueryParamsRef, RequestClient, RequestError},
};

pub type ReqwestClient = reqwest::Client;
#[cfg(feature = "high")]
pub type ReqwestMoonlightHost = crate::high::MoonlightHost<reqwest::Client>;

#[derive(Debug, Error)]
pub enum ReqwestError {
    #[error("{0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("{0}")]
    UrlParse(#[from] ParseError),
}
pub type ReqwestApiError = ApiError<ReqwestError>;

impl RequestError for ReqwestError {
    fn is_connect(&self) -> bool {
        matches!(self, ReqwestError::Reqwest(err) if err.is_connect() || err.is_timeout())
    }
    fn is_encryption(&self) -> bool {
        match self {
            ReqwestError::Reqwest(err) => err.is_decode(),
            _ => false,
        }
    }
}

fn default_builder() -> ClientBuilder {
    ClientBuilder::new()
        .use_native_tls()
        .timeout(DEFAULT_LONG_TIMEOUT)
        // https://github.com/seanmonstar/reqwest/issues/2021
        .pool_max_idle_per_host(0)
}
fn timeout_builder() -> ClientBuilder {
    default_builder().timeout(DEFAULT_TIMEOUT)
}

fn build_url(
    use_https: bool,
    hostport: &str,
    path: &str,
    query_params: &QueryParamsRef<'_>,
) -> Result<Url, ReqwestError> {
    let protocol = if use_https { "https" } else { "http" };

    let authority = format!("{protocol}://{hostport}/{path}");
    let url = Url::parse_with_params(&authority, query_params)?;

    Ok(url)
}

impl RequestClient for Client {
    type Error = ReqwestError;

    type Text = String;
    type Bytes = Bytes;

    fn with_defaults_long_timeout() -> Result<Self, Self::Error> {
        Ok(default_builder().build()?)
    }
    fn with_defaults() -> Result<Self, Self::Error> {
        Ok(timeout_builder().build()?)
    }

    fn with_certificates(
        client_private_key: &Pem,
        client_certificate: &Pem,
        server_certificate: &Pem,
    ) -> Result<Self, Self::Error> {
        let server_cert = Certificate::from_pem(server_certificate.to_string().as_bytes())?;

        let mut client_pem = String::new();
        client_pem.push_str(&client_private_key.to_string());
        client_pem.push('\n');
        client_pem.push_str(&client_certificate.to_string());

        let identity = Identity::from_pkcs8_pem(
            client_certificate.to_string().as_bytes(),
            client_private_key.to_string().as_bytes(),
        )?;

        Ok(timeout_builder()
            .tls_certs_only([server_cert])
            .identity(identity)
            .danger_accept_invalid_hostnames(true)
            .build()?)
    }

    #[instrument(skip(self), ret)]
    async fn send_http_request_text_response(
        &mut self,
        hostport: &str,
        path: &str,
        query_params: &QueryParamsRef<'_>,
    ) -> Result<Self::Text, Self::Error> {
        let url = build_url(false, hostport, path, query_params)?;

        let text = self.get(url).send().await?.text().await?;

        Ok(text)
    }

    #[instrument(skip(self), ret)]
    async fn send_https_request_text_response(
        &mut self,
        hostport: &str,
        path: &str,
        query_params: &QueryParamsRef<'_>,
    ) -> Result<Self::Text, Self::Error> {
        let url = build_url(true, hostport, path, query_params)?;

        let text = self.get(url).send().await?.text().await?;

        Ok(text)
    }

    #[instrument(skip(self), ret)]
    async fn send_https_request_data_response(
        &mut self,
        hostport: &str,
        path: &str,
        query_params: &QueryParamsRef<'_>,
    ) -> Result<Self::Bytes, Self::Error> {
        let url = build_url(true, hostport, path, query_params)?;

        let bytes = self.get(url).send().await?.bytes().await?;

        Ok(bytes)
    }
}
