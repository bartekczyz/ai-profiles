//! Talking to one remote host: HTTPS that trusts only the certificate the
//! host was paired with, a bearer token, and the host's addresses tried in
//! turn until one answers.

use std::time::Duration;

use ai_profiles_core::api::{ErrorBody, CLIENT_HEADER};
use ai_profiles_core::tls::pinned_client_config;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::hosts::RemoteHost;
use crate::error::{AppError, AppResult};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
// A restart waits for Claude to stop and then to come up again (up to 16s
// together on the server), so this leaves room above that.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct HostClient {
    http: reqwest::Client,
    host: RemoteHost,
    token: Option<String>,
    /// Longer than the usual timeout, for one slow request.
    timeout: Option<Duration>,
}

/// A successful answer, and which address gave it.
pub struct Answer<T> {
    pub value: T,
    pub address: String,
}

pub fn remote_error(code: &str, message: impl Into<String>) -> AppError {
    AppError::Remote {
        code: code.into(),
        message: message.into(),
    }
}

impl HostClient {
    /// `token` is `None` only for pairing, which is how a client gets one.
    pub fn new(host: RemoteHost, token: Option<String>) -> AppResult<HostClient> {
        let tls = pinned_client_config(&host.fingerprint).ok_or_else(|| {
            AppError::Validation(format!(
                "{} has a malformed certificate fingerprint",
                host.label
            ))
        })?;
        let http = reqwest::Client::builder()
            .use_preconfigured_tls(tls)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            // A redirect would carry the token somewhere the pin never
            // checked; the server never sends one.
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(format!("ai-profiles/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|err| AppError::Io(std::io::Error::other(err.to_string())))?;
        Ok(HostClient {
            http,
            host,
            token,
            timeout: None,
        })
    }

    /// The same client, giving its requests `timeout` rather than the usual
    /// one: for a request the host takes a while over, like a merge Claude
    /// writes.
    pub fn patient(mut self, timeout: Duration) -> HostClient {
        self.timeout = Some(timeout);
        self
    }

    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> AppResult<Answer<T>> {
        self.call(Method::GET, path, query, None::<&()>).await
    }

    pub async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> AppResult<Answer<T>> {
        self.call(Method::POST, path, &[], Some(body)).await
    }

    /// A DELETE that answers with a body.
    pub async fn delete_for<T: DeserializeOwned>(&self, path: &str) -> AppResult<Answer<T>> {
        self.call(Method::DELETE, path, &[], None::<&()>).await
    }

    /// A call whose answer has no body (204).
    pub async fn delete(&self, path: &str) -> AppResult<Answer<()>> {
        let (address, response) = self.send(Method::DELETE, path, &[], None::<&()>).await?;
        if response.status().is_success() {
            return Ok(Answer { value: (), address });
        }
        Err(self.error_from(response).await)
    }

    async fn call<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&B>,
    ) -> AppResult<Answer<T>> {
        let (address, response) = self.send(method, path, query, body).await?;
        if !response.status().is_success() {
            return Err(self.error_from(response).await);
        }
        let value = response.json::<T>().await.map_err(|_| {
            remote_error(
                "bad_response",
                format!("{} answered with something ai-profiles can't read. Is it running a matching ai-profiles-server?", self.host.label),
            )
        })?;
        Ok(Answer { value, address })
    }

    /// Send to each address in turn until one answers. A certificate that
    /// isn't the paired one stops the search at once: that address is
    /// someone else, and the next one could be too. So does a request that
    /// reached the host but wasn't answered in time, unless it only reads:
    /// the host may still be carrying it out, and sending it again (a move,
    /// say) would do it twice.
    async fn send<B: Serialize>(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&B>,
    ) -> AppResult<(String, reqwest::Response)> {
        for address in self.host.addresses_to_try() {
            let mut request = self
                .http
                .request(method.clone(), format!("https://{address}{path}"))
                .header(
                    CLIENT_HEADER,
                    format!("ai-profiles/{}", env!("CARGO_PKG_VERSION")),
                );
            if !query.is_empty() {
                request = request.query(query);
            }
            if let Some(token) = &self.token {
                request = request.bearer_auth(token);
            }
            if let Some(body) = body {
                request = request.json(body);
            }
            if let Some(timeout) = self.timeout {
                request = request.timeout(timeout);
            }
            match request.send().await {
                Ok(response) => return Ok((address, response)),
                Err(error) if is_certificate_mismatch(&error) => {
                    return Err(remote_error(
                        "cert_mismatch",
                        format!(
                            "{} presented a different certificate than the one it was paired with. If you reinstalled or reset the server, pair it again; otherwise something else is answering at {address}.",
                            self.host.label
                        ),
                    ));
                }
                Err(error) if error.is_connect() || method == Method::GET => continue,
                Err(error) if error.is_timeout() => {
                    return Err(remote_error(
                        "timeout",
                        format!(
                            "{} didn't answer in time. It may still be finishing: check before trying again.",
                            self.host.label
                        ),
                    ));
                }
                Err(_) => {
                    return Err(remote_error(
                        "no_answer",
                        format!(
                            "{} stopped answering partway. It may still have done it: check before trying again.",
                            self.host.label
                        ),
                    ));
                }
            }
        }
        Err(remote_error(
            "offline",
            format!(
                "{} can't be reached. Is it on, on Tailscale, and running ai-profiles-server?",
                self.host.label
            ),
        ))
    }

    async fn error_from(&self, response: reqwest::Response) -> AppError {
        let status = response.status();
        match response.json::<ErrorBody>().await {
            Ok(body) => remote_error(&body.error.code, body.error.message),
            Err(_) => remote_error(
                "bad_response",
                format!("{} answered {status}.", self.host.label),
            ),
        }
    }
}

/// Whether a request failed because the server's certificate isn't the
/// pinned one. reqwest wraps the TLS error a few layers down.
fn is_certificate_mismatch(error: &reqwest::Error) -> bool {
    let mut source: Option<&dyn std::error::Error> = Some(error);
    while let Some(current) = source {
        let text = current.to_string();
        if text.contains("invalid peer certificate")
            || text.contains("ApplicationVerificationFailure")
        {
            return true;
        }
        source = current.source();
    }
    false
}
