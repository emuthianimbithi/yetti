use crate::config::request_config::RequestConfig;
use reqwest::StatusCode;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryBackoff {
    Fixed,
    Exponential,
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub delay: Duration,
    pub backoff: RetryBackoff,
}

impl RetryPolicy {
    pub fn from_request(request: &RequestConfig) -> Self {
        let backoff = match request.retry_backoff.as_deref() {
            Some(value) if value.eq_ignore_ascii_case("fixed") => RetryBackoff::Fixed,
            _ => RetryBackoff::Exponential,
        };

        Self {
            max_retries: request.retry_attempts.unwrap_or(0),
            delay: Duration::from_secs(request.retry_delay_seconds.unwrap_or(1) as u64),
            backoff,
        }
    }

    pub fn delay_for_retry(self, retry_index: u32) -> Duration {
        match self.backoff {
            RetryBackoff::Fixed => self.delay,
            RetryBackoff::Exponential => {
                let multiplier = 2_u32.saturating_pow(retry_index.saturating_sub(1));
                self.delay.saturating_mul(multiplier)
            }
        }
    }
}

pub fn is_transient_status(status: StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_delay_doubles_each_retry() {
        let policy = RetryPolicy {
            max_retries: 3,
            delay: Duration::from_secs(2),
            backoff: RetryBackoff::Exponential,
        };

        assert_eq!(Duration::from_secs(2), policy.delay_for_retry(1));
        assert_eq!(Duration::from_secs(4), policy.delay_for_retry(2));
        assert_eq!(Duration::from_secs(8), policy.delay_for_retry(3));
    }

    #[test]
    fn only_expected_statuses_are_transient() {
        assert!(is_transient_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_transient_status(StatusCode::BAD_GATEWAY));
        assert!(!is_transient_status(StatusCode::BAD_REQUEST));
    }
}
