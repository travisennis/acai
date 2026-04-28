use std::time::Duration;

use anyhow::Error;
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use sha2::{Digest, Sha256};

pub const MAX_RETRIES: u32 = 5;
pub const BASE_DELAY_MS: u64 = 500;
pub const MAX_BACKOFF_MS: u64 = 30_000;

const CONTEXT_OVERFLOW_SAFETY_BUFFER_TOKENS: u32 = 1024;
const MIN_CONTEXT_OVERFLOW_OUTPUT_TOKENS: u32 = 256;
const JITTER_DIVISOR: u64 = 5;
const X_SHOULD_RETRY: &str = "x-should-retry";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestOverrides {
    pub max_output_tokens: Option<u32>,
    pub reasoning_max_tokens: Option<u32>,
    pub context_overflow_retry_used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryReason {
    RateLimit,
    Overloaded,
    ServerError,
    RequestTimeout,
    LockTimeout,
    Network,
    ContextOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryStatus {
    pub attempt: u32,
    pub max_retries: u32,
    pub delay: Duration,
    pub reason: RetryReason,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct HttpFailure {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryDecision {
    Retry {
        status: RetryStatus,
    },
    RetryWithOverrides {
        status: RetryStatus,
        overrides: RequestOverrides,
    },
    DoNotRetry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ContextOverflow {
    input_tokens: u32,
    requested_tokens: u32,
    context_limit: u32,
}

pub fn classify_http_failure(
    failure: &HttpFailure,
    attempt: u32,
    session_id: uuid::Uuid,
    current_overrides: &RequestOverrides,
) -> RetryDecision {
    if attempt >= MAX_RETRIES {
        return RetryDecision::DoNotRetry;
    }

    if let Some(status) = context_overflow_retry_status(failure, attempt, current_overrides) {
        return RetryDecision::RetryWithOverrides {
            status,
            overrides: apply_context_overflow_override(failure, current_overrides),
        };
    }

    let x_should_retry = parse_x_should_retry(&failure.headers);
    if x_should_retry == Some(false) {
        return RetryDecision::DoNotRetry;
    }

    let is_overloaded = has_overloaded_marker(&failure.body);

    match failure.status {
        408 => RetryDecision::Retry {
            status: retry_status(
                attempt,
                session_id,
                RetryReason::RequestTimeout,
                "408 request timeout".to_string(),
                &failure.headers,
            ),
        },
        409 => RetryDecision::Retry {
            status: retry_status(
                attempt,
                session_id,
                RetryReason::LockTimeout,
                "409 lock timeout".to_string(),
                &failure.headers,
            ),
        },
        429 => RetryDecision::Retry {
            status: retry_status(
                attempt,
                session_id,
                RetryReason::RateLimit,
                "429 rate limit".to_string(),
                &failure.headers,
            ),
        },
        529 => RetryDecision::Retry {
            status: retry_status(
                attempt,
                session_id,
                RetryReason::Overloaded,
                "529 overloaded provider".to_string(),
                &failure.headers,
            ),
        },
        500 | 502 | 503 | 504 => {
            let reason = if is_overloaded {
                RetryReason::Overloaded
            } else {
                RetryReason::ServerError
            };
            let detail = if reason == RetryReason::Overloaded {
                "overloaded provider".to_string()
            } else {
                format!("{} server error", failure.status)
            };

            RetryDecision::Retry {
                status: retry_status(attempt, session_id, reason, detail, &failure.headers),
            }
        },
        status if is_overloaded => RetryDecision::Retry {
            status: retry_status(
                attempt,
                session_id,
                RetryReason::Overloaded,
                if status == 400 {
                    "overloaded provider".to_string()
                } else {
                    format!("{status} overloaded provider")
                },
                &failure.headers,
            ),
        },
        status if x_should_retry == Some(true) && (500..600).contains(&status) => {
            RetryDecision::Retry {
                status: retry_status(
                    attempt,
                    session_id,
                    RetryReason::ServerError,
                    format!("{status} server error"),
                    &failure.headers,
                ),
            }
        },
        _ => RetryDecision::DoNotRetry,
    }
}

pub fn classify_transport_error(
    error: &Error,
    attempt: u32,
    session_id: uuid::Uuid,
) -> RetryDecision {
    if attempt >= MAX_RETRIES {
        return RetryDecision::DoNotRetry;
    }

    let Some(detail) = transport_retry_detail(error) else {
        return RetryDecision::DoNotRetry;
    };

    RetryDecision::Retry {
        status: RetryStatus {
            attempt: attempt + 1,
            max_retries: MAX_RETRIES,
            delay: fallback_delay(attempt, session_id),
            reason: RetryReason::Network,
            detail,
        },
    }
}

pub fn should_disable_connection_reuse(error: &Error) -> bool {
    transport_retry_detail(error).is_some()
}

fn context_overflow_retry_status(
    failure: &HttpFailure,
    attempt: u32,
    current_overrides: &RequestOverrides,
) -> Option<RetryStatus> {
    if failure.status != 400 || current_overrides.context_overflow_retry_used {
        return None;
    }

    let overflow = parse_context_overflow(&failure.body)?;
    let available_output = overflow
        .context_limit
        .saturating_sub(overflow.input_tokens)
        .saturating_sub(CONTEXT_OVERFLOW_SAFETY_BUFFER_TOKENS);

    if available_output < MIN_CONTEXT_OVERFLOW_OUTPUT_TOKENS {
        return None;
    }

    let max_output_tokens = available_output.min(overflow.requested_tokens);

    Some(RetryStatus {
        attempt: attempt + 1,
        max_retries: MAX_RETRIES,
        delay: Duration::ZERO,
        reason: RetryReason::ContextOverflow,
        detail: format!("max_output_tokens={max_output_tokens}"),
    })
}

fn apply_context_overflow_override(
    failure: &HttpFailure,
    current_overrides: &RequestOverrides,
) -> RequestOverrides {
    let Some(overflow) = parse_context_overflow(&failure.body) else {
        return current_overrides.clone();
    };

    let available_output = overflow
        .context_limit
        .saturating_sub(overflow.input_tokens)
        .saturating_sub(CONTEXT_OVERFLOW_SAFETY_BUFFER_TOKENS);
    let max_output_tokens = available_output.min(overflow.requested_tokens);

    RequestOverrides {
        max_output_tokens: Some(max_output_tokens),
        reasoning_max_tokens: current_overrides
            .reasoning_max_tokens
            .map(|budget| budget.min(max_output_tokens.saturating_sub(1))),
        context_overflow_retry_used: true,
    }
}

fn retry_status(
    attempt: u32,
    session_id: uuid::Uuid,
    reason: RetryReason,
    detail: String,
    headers: &HeaderMap,
) -> RetryStatus {
    RetryStatus {
        attempt: attempt + 1,
        max_retries: MAX_RETRIES,
        delay: parse_retry_after(headers).unwrap_or_else(|| fallback_delay(attempt, session_id)),
        reason,
        detail,
    }
}

fn fallback_delay(attempt: u32, session_id: uuid::Uuid) -> Duration {
    let base_delay_ms = capped_backoff_ms(attempt);
    let jitter_bound_ms = if base_delay_ms >= MAX_BACKOFF_MS {
        0
    } else {
        (base_delay_ms / JITTER_DIVISOR).max(1)
    };
    let jitter_ms = deterministic_jitter_ms(session_id, attempt, jitter_bound_ms);
    Duration::from_millis(base_delay_ms.saturating_add(jitter_ms))
}

fn capped_backoff_ms(attempt: u32) -> u64 {
    let exponent = attempt.saturating_sub(1).min(31);
    BASE_DELAY_MS
        .saturating_mul(1_u64 << exponent)
        .min(MAX_BACKOFF_MS)
}

fn deterministic_jitter_ms(session_id: uuid::Uuid, attempt: u32, max_jitter_ms: u64) -> u64 {
    if max_jitter_ms == 0 {
        return 0;
    }

    let mut hasher = Sha256::new();
    hasher.update(session_id.as_bytes());
    hasher.update(attempt.to_be_bytes());
    let digest = hasher.finalize();

    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(bytes) % (max_jitter_ms + 1)
}

fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    parse_retry_after_at(headers, Utc::now())
}

fn parse_retry_after_at(headers: &HeaderMap, now: DateTime<Utc>) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();

    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    let retry_at = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    let delay = retry_at.signed_duration_since(now);

    if delay <= chrono::Duration::zero() {
        return Some(Duration::ZERO);
    }

    delay.to_std().ok()
}

fn parse_x_should_retry(headers: &HeaderMap) -> Option<bool> {
    let value = headers.get(X_SHOULD_RETRY)?.to_str().ok()?.trim();

    if value.eq_ignore_ascii_case("true") {
        Some(true)
    } else if value.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

fn has_overloaded_marker(body: &str) -> bool {
    body.to_ascii_lowercase().contains("overloaded_error")
}

fn parse_context_overflow(body: &str) -> Option<ContextOverflow> {
    let owned_message = extract_error_message(body);
    let message = owned_message.as_deref().unwrap_or(body);
    let message_lower = message.to_ascii_lowercase();

    if !(message_lower.contains("context limit") && message_lower.contains("max_tokens")) {
        return None;
    }

    let marker = message_lower.find("context limit")?;
    let expression = &message[marker + "context limit".len()..];
    let plus = expression.find('+')?;
    let greater_than = plus + 1 + expression[plus + 1..].find('>')?;

    let input_tokens = parse_last_u32(&expression[..plus])?;
    let requested_tokens = parse_first_u32(&expression[plus + 1..greater_than])?;
    let context_limit = parse_first_u32(&expression[greater_than + 1..])?;

    Some(ContextOverflow {
        input_tokens,
        requested_tokens,
        context_limit,
    })
}

fn extract_error_message(body: &str) -> Option<String> {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok()?;

    parsed
        .pointer("/error/message")
        .and_then(serde_json::Value::as_str)
        .or_else(|| parsed.get("message").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
}

fn extract_u32_values(input: &str) -> Vec<u32> {
    let mut values = Vec::new();
    let mut digits = String::new();

    for ch in input.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }

        if !digits.is_empty() {
            if let Ok(value) = digits.parse::<u32>() {
                values.push(value);
            }
            digits.clear();
        }
    }

    if !digits.is_empty()
        && let Ok(value) = digits.parse::<u32>()
    {
        values.push(value);
    }

    values
}

fn parse_first_u32(input: &str) -> Option<u32> {
    extract_u32_values(input).into_iter().next()
}

fn parse_last_u32(input: &str) -> Option<u32> {
    extract_u32_values(input).into_iter().last()
}

fn transport_retry_detail(error: &Error) -> Option<String> {
    for cause in error.chain() {
        if let Some(reqwest_error) = cause.downcast_ref::<reqwest::Error>() {
            if reqwest_error.is_timeout() {
                return Some("stale connection timeout".to_string());
            }
            if reqwest_error.is_connect() {
                return Some("stale connection failure".to_string());
            }
        }
    }

    let lower = error
        .chain()
        .map(|cause| cause.to_string().to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" | ");

    if lower.contains("connection reset") {
        Some("stale connection reset".to_string())
    } else if lower.contains("broken pipe") {
        Some("stale broken pipe".to_string())
    } else if lower.contains("unexpected eof") || lower.contains("end of file") {
        Some("stale unexpected eof".to_string())
    } else if lower.contains("timed out") {
        Some("stale connection timeout".to_string())
    } else {
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use reqwest::header::HeaderValue;

    fn session_id() -> uuid::Uuid {
        uuid::uuid!("550e8400-e29b-41d4-a716-446655440000")
    }

    #[test]
    fn parse_retry_after_delta_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("3"));

        assert_eq!(
            parse_retry_after_at(&headers, Utc::now()),
            Some(Duration::from_secs(3))
        );
    }

    #[test]
    fn parse_retry_after_http_date() {
        let now = Utc.with_ymd_and_hms(2026, 4, 28, 0, 0, 0).single().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Tue, 28 Apr 2026 00:01:30 GMT"),
        );

        assert_eq!(
            parse_retry_after_at(&headers, now),
            Some(Duration::from_secs(90))
        );
    }

    #[test]
    fn x_should_retry_false_blocks_retry() {
        let mut headers = HeaderMap::new();
        headers.insert(X_SHOULD_RETRY, HeaderValue::from_static("false"));
        let failure = HttpFailure {
            status: 503,
            headers,
            body: r#"{"error":{"message":"busy","type":"server_error"}}"#.to_string(),
        };

        assert_eq!(
            classify_http_failure(&failure, 1, session_id(), &RequestOverrides::default()),
            RetryDecision::DoNotRetry
        );
    }

    #[test]
    fn x_should_retry_false_blocks_overloaded_error() {
        let mut headers = HeaderMap::new();
        headers.insert(X_SHOULD_RETRY, HeaderValue::from_static("false"));
        let failure = HttpFailure {
            status: 503,
            headers,
            body: r#"{"error":{"message":"provider overloaded","type":"overloaded_error"}}"#
                .to_string(),
        };

        assert_eq!(
            classify_http_failure(&failure, 1, session_id(), &RequestOverrides::default()),
            RetryDecision::DoNotRetry
        );
    }

    #[test]
    fn parse_context_overflow_reduces_output_budget() {
        let failure = HttpFailure {
            status: 400,
            headers: HeaderMap::new(),
            body: r#"{"error":{"message":"input length and max_tokens exceed context limit: 12000 + 5000 > 16384"}}"#.to_string(),
        };
        let decision = classify_http_failure(
            &failure,
            1,
            session_id(),
            &RequestOverrides {
                max_output_tokens: Some(5000),
                reasoning_max_tokens: Some(4000),
                context_overflow_retry_used: false,
            },
        );

        match decision {
            RetryDecision::RetryWithOverrides { status, overrides } => {
                assert_eq!(status.reason, RetryReason::ContextOverflow);
                assert_eq!(status.delay, Duration::ZERO);
                assert_eq!(overrides.max_output_tokens, Some(3360));
                assert_eq!(overrides.reasoning_max_tokens, Some(3359));
                assert!(overrides.context_overflow_retry_used);
            },
            other => panic!("expected overflow retry override, got {other:?}"),
        }
    }

    #[test]
    fn parse_context_overflow_ignores_trailing_numeric_metadata() {
        let failure = HttpFailure {
            status: 400,
            headers: HeaderMap::new(),
            body: r#"{"error":{"message":"input length and max_tokens exceed context limit: 12000 + 5000 > 16384 (request 42)"}}"#.to_string(),
        };

        match classify_http_failure(
            &failure,
            1,
            session_id(),
            &RequestOverrides {
                max_output_tokens: Some(5000),
                reasoning_max_tokens: Some(4000),
                context_overflow_retry_used: false,
            },
        ) {
            RetryDecision::RetryWithOverrides { overrides, .. } => {
                assert_eq!(overrides.max_output_tokens, Some(3360));
                assert_eq!(overrides.reasoning_max_tokens, Some(3359));
            },
            other => panic!("expected overflow retry override, got {other:?}"),
        }
    }

    #[test]
    fn fallback_delay_caps_and_jitters() {
        let capped = fallback_delay(10, session_id());
        assert_eq!(capped, Duration::from_millis(MAX_BACKOFF_MS));

        let jittered = fallback_delay(3, session_id());
        assert!(jittered >= Duration::from_secs(2));
        assert!(jittered <= Duration::from_millis(2_400));
    }

    #[test]
    fn stale_connection_markers_retry_and_disable_reuse() {
        let error = anyhow::anyhow!("connection reset by peer");

        assert!(should_disable_connection_reuse(&error));
        match classify_transport_error(&error, 1, session_id()) {
            RetryDecision::Retry { status } => {
                assert_eq!(status.reason, RetryReason::Network);
                assert_eq!(status.detail, "stale connection reset");
            },
            other => panic!("expected transport retry, got {other:?}"),
        }
    }
}
