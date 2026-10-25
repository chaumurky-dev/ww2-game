use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use wartide_protocol::ApiError;

/// The single error type for handlers and services. Renders as [`ApiError`] JSON.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{1}")]
    BadRequest(&'static str, String),
    #[error("authentication required")]
    Unauthorized,
    #[error("email address not verified")]
    EmailUnverified,
    #[error("{0}")]
    Forbidden(String),
    #[error("{1}")]
    ForbiddenCode(&'static str, String),
    #[error("{0}")]
    NotFound(String),
    #[error("{1}")]
    Conflict(&'static str, String),
    #[error("too many requests; try again in {0}s")]
    TooManyRequests(u64),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn bad(msg: impl Into<String>) -> Self {
        AppError::BadRequest("bad_request", msg.into())
    }
    pub fn not_found(what: &str) -> Self {
        AppError::NotFound(format!("{what} not found"))
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        AppError::Forbidden(msg.into())
    }
    pub fn conflict(code: &'static str, msg: impl Into<String>) -> Self {
        AppError::Conflict(code, msg.into())
    }

    pub fn status(&self) -> StatusCode {
        match self {
            AppError::BadRequest(..) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::EmailUnverified | AppError::Forbidden(_) | AppError::ForbiddenCode(..) => {
                StatusCode::FORBIDDEN
            }
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(..) => StatusCode::CONFLICT,
            AppError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &str {
        match self {
            AppError::BadRequest(c, _)
            | AppError::Conflict(c, _)
            | AppError::ForbiddenCode(c, _) => c,
            AppError::Unauthorized => "unauthorized",
            AppError::EmailUnverified => "email_unverified",
            AppError::Forbidden(_) => "forbidden",
            AppError::NotFound(_) => "not_found",
            AppError::TooManyRequests(_) => "rate_limited",
            AppError::Internal(_) => "internal",
        }
    }

    pub fn to_api(&self) -> ApiError {
        let message = match self {
            // don't leak internals
            AppError::Internal(_) => "internal server error".to_string(),
            other => other.to_string(),
        };
        ApiError {
            code: self.code().to_string(),
            message,
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let AppError::Internal(e) = &self {
            tracing::error!(error = ?e, "internal error");
        }
        let retry = match &self {
            AppError::TooManyRequests(secs) => Some(*secs),
            _ => None,
        };
        let mut res = (self.status(), Json(self.to_api())).into_response();
        if let Some(secs) = retry {
            res.headers_mut()
                .insert(axum::http::header::RETRY_AFTER, secs.into());
        }
        res
    }
}

/// True if the sqlx error is a unique-constraint violation.
pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(d) if d.is_unique_violation())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_status() {
        assert_eq!(AppError::EmailUnverified.code(), "email_unverified");
        assert_eq!(AppError::EmailUnverified.status(), StatusCode::FORBIDDEN);
        let e = AppError::Internal(anyhow::anyhow!("secret db detail"));
        assert_eq!(e.to_api().message, "internal server error");
    }
}
