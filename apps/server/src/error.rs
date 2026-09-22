use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
pub use contracts::http::{FieldError, Problem};
use contracts::{Id, Revision};
use store::StoreError;
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    detail: &'static str,
    current_revision: Option<Revision>,
    field_errors: Vec<FieldError>,
}
impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, detail: &'static str) -> Self {
        Self {
            status,
            code,
            detail,
            current_revision: None,
            field_errors: Vec::new(),
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
            "本机会话已失效，请刷新页面。",
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
            title: self.code.to_owned(),
            status: self.status.as_u16(),
            code: self.code.to_owned(),
            detail: self.detail.to_owned(),
            request_id,
            current_revision: self.current_revision,
            field_errors: self.field_errors,
            safe_next_actions: match self.code {
                "AUTH_REQUIRED" => vec!["RELOAD".into()],
                "REVISION_CONFLICT" => vec!["RELOAD".into()],
                _ => Vec::new(),
            },
            retryable: self.code != "BUDGET_EXHAUSTED"
                && (self.status == StatusCode::SERVICE_UNAVAILABLE
                    || self.status == StatusCode::TOO_MANY_REQUESTS),
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
        response
    }
}
/// Preserve native capacity failure without exposing filesystem paths or bytes.
pub(crate) fn artifact_storage(error: integrations::artifacts::ArtifactError) -> StoreError {
    if matches!(error, integrations::artifacts::ArtifactError::Io(ref error)
        if error.kind() == std::io::ErrorKind::StorageFull)
    {
        tracing::error!(
            code = "STORAGE_FULL",
            "artifact publication unavailable: storage full"
        );
        StoreError::StorageFull
    } else {
        StoreError::Integrity
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::AuthenticationRequired => Self::authentication(),
            StoreError::InvalidCredentials => Self::new(
                StatusCode::UNAUTHORIZED,
                "AUTHENTICATION_FAILED",
                "机器凭据无效或已失效。",
            ),
            StoreError::Forbidden => Self::new(
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                "当前身份没有执行此操作的权限。",
            ),
            StoreError::RevisionConflict { current } => {
                let mut error = Self::new(
                    StatusCode::CONFLICT,
                    "REVISION_CONFLICT",
                    "记录已被其他请求更新，请载入当前版本后重试。",
                );
                error.current_revision = Some(current);
                error
            }
            StoreError::IdempotencyConflict => Self::new(
                StatusCode::CONFLICT,
                "IDEMPOTENCY_CONFLICT",
                "此幂等键已用于不同请求，不能重用。",
            ),
            StoreError::NativeIdentityConflict => Self::new(
                StatusCode::CONFLICT,
                "NATIVE_IDENTITY_CONFLICT",
                "这个原生身份已绑定其他不可变内容，不能通过更换标识覆盖来源或授权。",
            ),
            StoreError::StorageFull => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "STORAGE_FULL",
                "产物存储空间已满。请联系运维恢复可用空间，再使用原请求编号重试；不要删除仍被引用的产物。",
            ),
            StoreError::IntegrationUnavailable => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "INTEGRATION_UNAVAILABLE",
                "已登记的外部集成暂不可用，请检查其连接、凭据和运行状态后使用原请求重试。",
            ),
            StoreError::EventCursorExpired => Self::new(
                StatusCode::GONE,
                "EVENT_CURSOR_EXPIRED",
                "事件游标不可继续使用，请重新载入运行快照。",
            ),
            StoreError::EventContractUnsupported => Self::new(
                StatusCode::CONFLICT,
                "CONTRACT_VERSION_UNSUPPORTED",
                "持久事件合同不受当前客户端版本支持。",
            ),
            StoreError::Integrity | StoreError::SecretCleanup => Self::internal(),
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
            StoreError::Domain(domain::DomainError::Fields(fields)) => {
                let mut error = Self::validation();
                error.field_errors = fields
                    .into_iter()
                    .map(|f| FieldError {
                        field: f.field,
                        code: f.code,
                        message: f.message,
                    })
                    .collect();
                error
            }
            StoreError::Domain(domain::DomainError::Invalid(_)) => Self::validation(),
            StoreError::Domain(domain::DomainError::BudgetExhausted(resource)) => {
                let mut error = Self::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    "BUDGET_EXHAUSTED",
                    "请求超过已冻结的资源预算。请检查运行预算；不要自动重试或更换任务来绕过额度。",
                );
                // Domain errors carry internal tags; only this closed vocabulary
                // is public. Never reflect a future tag containing private data.
                let field = match resource {
                    "artifact_output_bytes"
                    | "experiments"
                    | "cpu_seconds"
                    | "parallel_runs"
                    | "standalone_parallel_runs"
                    | "tokens"
                    | "estimated_cost"
                    | "mission_turns"
                    | "repair_turns"
                    | "job_resource_limit" => resource,
                    _ => "budget",
                };
                error.field_errors.push(FieldError {
                    field: field.to_owned(),
                    code: "BUDGET_EXHAUSTED".to_owned(),
                    message: "该资源的冻结预算不足。".to_owned(),
                });
                error
            }
            StoreError::Domain(_) => Self::new(
                StatusCode::CONFLICT,
                "DOMAIN_CONFLICT",
                "当前领域状态不允许该操作。",
            ),
            StoreError::Database(_) | StoreError::Migration(_) => Self::internal(),
        }
    }
}
