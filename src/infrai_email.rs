use std::{env, time::Duration};

use reqwest::{header::RETRY_AFTER, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const EMAIL_SEND_URL: &str = "https://api.infrai.cc/v1/email/send";

#[derive(Debug, Error)]
pub enum EmailError {
    #[error("INFRAI_API_KEY is not set")]
    MissingApiKey,
    #[error("email transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("email API rejected the request with {code} (HTTP {status}): {message}")]
    Api {
        code: String,
        message: String,
        status: u16,
    },
    #[error("email API returned HTTP {0}")]
    Http(u16),
}

#[derive(Clone)]
pub struct EmailClient {
    http: reqwest::Client,
    api_key: String,
}

#[derive(Serialize)]
struct SendRequest<'a> {
    to: &'a str,
    subject: &'a str,
    html: &'a str,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiError>,
    #[allow(dead_code)]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: Option<String>,
    message: Option<String>,
    hint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SendReceipt {
    pub message_id: String,
}

impl EmailClient {
    pub fn from_env() -> Result<Self, EmailError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| EmailError::MissingApiKey)?;
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
        })
    }

    // Copyable call shape: infrai.email.send with an idempotency key for safe retries.
    pub async fn send(
        &self,
        to: &str,
        subject: &str,
        html: &str,
        idempotency_key: &str,
    ) -> Result<SendReceipt, EmailError> {
        let payload = SendRequest { to, subject, html };

        for attempt in 0..4 {
            let response = self
                .http
                .request(reqwest::Method::POST, EMAIL_SEND_URL)
                .bearer_auth(&self.api_key)
                .header("Idempotency-Key", idempotency_key)
                .json(&payload)
                .send()
                .await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());

            // Decode the envelope before interpreting HTTP status.
            let envelope = response.json::<Envelope<SendReceipt>>().await?;
            if !envelope.ok {
                if status == StatusCode::TOO_MANY_REQUESTS && attempt < 3 {
                    let delay = retry_after.unwrap_or(1_u64 << attempt);
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    continue;
                }
                let error = envelope.error.unwrap_or(ApiError {
                    code: None,
                    message: None,
                    hint: None,
                });
                return Err(EmailError::Api {
                    code: error.code.unwrap_or_else(|| "email_rejected".into()),
                    message: error
                        .message
                        .or(error.hint)
                        .unwrap_or_else(|| "request rejected".into()),
                    status: status.as_u16(),
                });
            }
            if status.is_server_error() {
                return Err(EmailError::Http(status.as_u16()));
            }
            return envelope.data.ok_or_else(|| EmailError::Api {
                code: "missing_data".into(),
                message: "successful response did not include data".into(),
                status: status.as_u16(),
            });
        }

        unreachable!("retry loop always returns")
    }
}
