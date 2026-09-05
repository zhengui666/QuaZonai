use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use contracts::{Id, Revision};
use serde::Serialize;
use store::StoreError;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct FieldError {
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Problem {
    #[serde(rename = "type")]
    pub kind: String,
    pub title: &'static str,
    #[schema(minimum = 100, maximum = 599)]
    pub status: u16,
    pub code: &'static str,
    pub detail: &'static str,
    pub request_id: Id,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<Revision>,
    pub field_errors: Vec<FieldError>,
    pub safe_next_actions: Vec<&'static str>,
}
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    detail: &'static str,
    retry_after: Option<u32>,
}
impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, detail: &'static str) -> Self {
        Self {
            status,
            code,
            detail,
            retry_after: None,
        }
    }
    pub fn internal() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE",
            "服务暂时无法完成请求。请使用请求编号检查运行日志。",
        )
    }
    pub fn validation() -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "请求字段缺失、类型错误或不符合当前接口合同。",
        )
    }
    pub fn authentication() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "AUTH_REQUIRED",
            "请使用验证器重新登录。",
        )
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let request_id = Id::new();
        if self.status.is_server_error() {
            tracing::error!(%request_id,code=self.code,"request failed");
        }
        let problem = Problem {
            kind: format!(
                "urn:quazonai:problem:{}",
                self.code.to_ascii_lowercase().replace('_', "-")
            ),
            title: self.code,
            status: self.status.as_u16(),
            code: self.code,
            detail: self.detail,
            request_id,
            current_revision: None,
            field_errors: Vec::new(),
            safe_next_actions: match self.code {
                "AUTH_REQUIRED" | "RECENT_AUTH_REQUIRED" => vec!["AUTHENTICATE"],
                "AUTH_RATE_LIMITED" => vec!["RETRY_AFTER"],
                "REVISION_CONFLICT" => vec!["RELOAD"],
                _ => Vec::new(),
            },
            retryable: self.status == StatusCode::SERVICE_UNAVAILABLE
                || self.status == StatusCode::TOO_MANY_REQUESTS,
        };
        let mut response = (self.status, Json(problem)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response.headers_mut().insert(
            "x-request-id",
            HeaderValue::from_str(&request_id.to_string()).expect("UUID header"),
        );
        if let Some(seconds) = self.retry_after {
            response.headers_mut().insert(
                header::RETRY_AFTER,
                HeaderValue::from_str(&seconds.to_string()).expect("integer header"),
            );
        }
        response
    }
}
impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::AuthenticationRequired => Self::authentication(),
            StoreError::InvalidCredentials => Self::new(
                StatusCode::UNAUTHORIZED,
                "AUTHENTICATION_FAILED",
                "验证码或初始化凭据无效、过期或已使用。",
            ),
            StoreError::SetupCompleted => Self::new(
                StatusCode::CONFLICT,
                "SETUP_ALREADY_COMPLETED",
                "系统已经完成初始化，不能重新绑定验证器。",
            ),
            StoreError::TotpReplay => Self::new(
                StatusCode::CONFLICT,
                "TOTP_REPLAY",
                "这个时间步的验证码已使用，请使用验证器生成的新验证码。",
            ),
            StoreError::RecentAuthenticationRequired => Self::new(
                StatusCode::FORBIDDEN,
                "RECENT_AUTH_REQUIRED",
                "此敏感操作需要重新验证一次动态码。",
            ),
            StoreError::AuthRateLimited {
                retry_after_seconds,
            } => {
                let mut error = Self::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    "AUTH_RATE_LIMITED",
                    "验证尝试过于频繁，请依据 Retry-After 重新尝试。",
                );
                error.retry_after = Some(retry_after_seconds);
                error
            }
            StoreError::NotFound => Self::new(
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                "请求的记录不存在或不可访问。",
            ),
            StoreError::Conflict => Self::new(
                StatusCode::CONFLICT,
                "REVISION_CONFLICT",
                "记录或不可变请求发生冲突，请重新载入后检查。",
            ),
            StoreError::Invalid(_) => Self::validation(),
            StoreError::TurnPending => Self::new(
                StatusCode::CONFLICT,
                "TURN_PENDING",
                "此前模型请求仍待确认，不能重复发送。",
            ),
            StoreError::Domain(_) => Self::new(
                StatusCode::CONFLICT,
                "DOMAIN_CONFLICT",
                "当前领域状态不允许该操作。",
            ),
            StoreError::Database(_) | StoreError::Migration(_) => Self::internal(),
        }
    }
}
