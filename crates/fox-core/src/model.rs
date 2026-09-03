//! 领域模型：与 SPEC 第 8~12 节保持一致。

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;

/// HTTP 方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
}

impl HttpMethod {
    pub fn all() -> &'static [HttpMethod] {
        &[
            HttpMethod::GET,
            HttpMethod::POST,
            HttpMethod::PUT,
            HttpMethod::DELETE,
            HttpMethod::PATCH,
            HttpMethod::HEAD,
            HttpMethod::OPTIONS,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::GET => "GET",
            HttpMethod::POST => "POST",
            HttpMethod::PUT => "PUT",
            HttpMethod::DELETE => "DELETE",
            HttpMethod::PATCH => "PATCH",
            HttpMethod::HEAD => "HEAD",
            HttpMethod::OPTIONS => "OPTIONS",
        }
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for HttpMethod {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(HttpMethod::GET),
            "POST" => Ok(HttpMethod::POST),
            "PUT" => Ok(HttpMethod::PUT),
            "DELETE" => Ok(HttpMethod::DELETE),
            "PATCH" => Ok(HttpMethod::PATCH),
            "HEAD" => Ok(HttpMethod::HEAD),
            "OPTIONS" => Ok(HttpMethod::OPTIONS),
            other => Err(AppError::Validation(format!("不支持的 HTTP 方法：{other}"))),
        }
    }
}

/// 接口状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EndpointStatus {
    Designing,
    #[default]
    Developing,
    Testing,
    Released,
    Deprecated,
}

impl EndpointStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            EndpointStatus::Designing => "designing",
            EndpointStatus::Developing => "developing",
            EndpointStatus::Testing => "testing",
            EndpointStatus::Released => "released",
            EndpointStatus::Deprecated => "deprecated",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            EndpointStatus::Designing => "设计中",
            EndpointStatus::Developing => "开发中",
            EndpointStatus::Testing => "测试中",
            EndpointStatus::Released => "已发布",
            EndpointStatus::Deprecated => "已废弃",
        }
    }
}

/// 参数字段类型（接口设计页的 Schema 标注；旧数据缺省为 String）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    #[default]
    String,
    Number,
    Boolean,
    Object,
}

/// 键值对：用于 Query / Header / Path 变量等。
///
/// `field_type` / `required` / `example` 为接口设计的参数元数据，
/// serde 带默认值：旧持久化 JSON 缺省这些字段时照常反序列化。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub field_type: FieldType,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default)]
    pub example: String,
}

fn default_fault_status() -> u16 {
    500
}

fn default_true() -> bool {
    true
}

impl KeyValue {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        KeyValue {
            key: key.into(),
            value: value.into(),
            enabled: true,
            description: String::new(),
            field_type: FieldType::default(),
            required: true,
            example: String::new(),
        }
    }
}

/// API Key 放置位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    Header,
    Query,
}

/// 认证方式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthSpec {
    #[default]
    None,
    Bearer {
        token: String,
    },
    Basic {
        username: String,
        password: String,
    },
    #[serde(rename = "apikey")]
    ApiKey {
        key: String,
        value: String,
        #[serde(rename = "in")]
        location: ApiKeyLocation,
    },
    /// OAuth2 授权码流（Authorization Code Grant）。
    ///
    /// 浏览器跳转 `auth_url` 完成授权 → 本地回调服务器（127.0.0.1:9090）收到 code →
    /// `token_url` 换取 access_token / refresh_token → 存入 `token`。
    /// 过期后由 `fox-oauth` 用 refresh_token 静默刷新。
    #[serde(rename = "oauth2")]
    OAuth2 {
        client_id: String,
        client_secret: String,
        /// 授权页地址（GET，追加 response_type=code / state / redirect_uri / scope）。
        auth_url: String,
        /// 换取与刷新 token 的地址（POST，form 编码）。
        token_url: String,
        scope: String,
        /// 回调地址，须与授权服务注册的一致（默认 http://127.0.0.1:9090/callback）。
        redirect_uri: String,
        /// 已获取的令牌；授权成功后写入，刷新后由缓存回填。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token: Option<OAuth2Token>,
    },
    /// HTTP Digest 认证（RFC 7616）。
    ///
    /// 发送时先不带凭据；收到 `401 + WWW-Authenticate: Digest …` 后按质询
    /// 计算应答并自动重发一次（支持 MD5 / MD5-sess / SHA-256，qop=auth）。
    Digest {
        username: String,
        password: String,
    },
    /// Hawk 认证（Hawk 协议，HMAC-SHA-256）。
    ///
    /// 每次发送用递增时间戳 + 随机 nonce 计算 `mac`，有 body 时附带 payload `hash`。
    Hawk {
        /// 凭证标识（`id`）。
        key_id: String,
        /// 共享密钥（`key`）。
        key: String,
    },
    /// AWS Signature V4（兼容华为云 / 阿里云等 SigV4 风格网关）。
    ///
    /// 按区域 + 服务名做四步密钥派生，对方法 / 路径 / 排序后查询串 /
    /// `host;x-amz-date[;x-amz-security-token]` 头做规范签名。
    #[serde(rename = "awsv4")]
    AwsV4 {
        access_key: String,
        secret_key: String,
        region: String,
        service: String,
        /// 临时凭证配套；为空时不发送 `x-amz-security-token`。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_token: Option<String>,
    },
    /// 通用 HMAC AK-SK 加签（时间戳 + 随机数 + 方法 + 路径）。
    ///
    /// 发送 `X-Access-Key / X-Timestamp / X-Nonce / X-Signature` 四个头，
    /// `X-Signature = hex(HMAC-SHA256(secret, "ak\\nts\\nnonce\\nMETHOD\\npath_query"))`。
    #[serde(rename = "hmac")]
    Hmac {
        access_key: String,
        secret_key: String,
    },
}

/// OAuth2 令牌（access + refresh + 过期时刻）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuth2Token {
    pub access_token: String,
    #[serde(default = "default_token_type")]
    pub token_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// 过期时刻（UTC）。
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

impl OAuth2Token {
    /// 已过期（`expires_at` 早于当前时间）。
    pub fn is_expired(&self) -> bool {
        self.expires_at <= chrono::Utc::now()
    }

    /// 距过期不足 `threshold` 视为即将过期（用于提前静默刷新）。
    pub fn expires_within(&self, threshold: chrono::Duration) -> bool {
        self.expires_at - chrono::Utc::now() <= threshold
    }
}

/// OAuth2 授权状态（供 UI 状态指示器）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuth2Status {
    /// 未授权（无 token 或无 refresh_token）。
    Unauthorized,
    /// 已授权，token 有效且未临近过期。
    Valid,
    /// 即将过期（≤ 60 秒），请求时将自动刷新。
    ExpiringSoon,
    /// 已过期（但仍有 refresh_token，请求时将自动刷新）。
    Expired,
}

impl AuthSpec {
    /// 若为 OAuth2，返回其授权状态。
    pub fn oauth2_status(&self) -> Option<OAuth2Status> {
        let AuthSpec::OAuth2 { token, .. } = self else {
            return None;
        };
        let Some(token) = token else {
            return Some(OAuth2Status::Unauthorized);
        };
        if token.is_expired() {
            return Some(OAuth2Status::Expired);
        }
        if token.expires_within(chrono::Duration::seconds(60)) {
            return Some(OAuth2Status::ExpiringSoon);
        }
        Some(OAuth2Status::Valid)
    }
}

/// Multipart 值类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultipartValueType {
    Text,
    FilePath,
}

/// Multipart 字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartField {
    pub key: String,
    pub value_type: MultipartValueType,
    pub value: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// GraphQL 请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GraphQLSpec {
    pub query: String,
    /// 变量 JSON 文本（空串或 "{}" 表示无变量）。
    pub variables: String,
    pub operation_name: String,
}

/// GraphQL 错误位置（errors[].locations[]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphQLErrorLocation {
    pub line: u64,
    pub column: u64,
}

/// GraphQL 错误条目（errors[]）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphQLError {
    pub message: String,
    pub locations: Option<Vec<GraphQLErrorLocation>>,
    pub path: Option<Vec<serde_json::Value>>,
}

/// 解析后的 GraphQL 响应：`data` 与 `errors` 并存或互斥。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphQLResponse {
    pub data: Option<serde_json::Value>,
    pub errors: Vec<GraphQLError>,
}

impl GraphQLResponse {
    /// 是否存在错误。
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// 请求 Body。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum BodySpec {
    #[default]
    None,
    Json {
        raw: String,
    },
    Text {
        raw: String,
    },
    #[serde(rename = "urlencoded")]
    UrlEncoded {
        fields: Vec<KeyValue>,
    },
    Multipart {
        fields: Vec<MultipartField>,
    },
    GraphQL {
        spec: GraphQLSpec,
    },
    /// 二进制文件请求体：发送时由执行器读取 `path` 指向文件的原始字节，
    /// Content-Type 默认 application/octet-stream（用户显式设置的请求头优先）。
    Binary {
        path: String,
    },
}

impl BodySpec {
    /// 当前 body 模式的内部名称（用于 UI 下拉框）。
    pub fn mode_name(&self) -> &'static str {
        match self {
            BodySpec::None => "none",
            BodySpec::Json { .. } => "json",
            BodySpec::Text { .. } => "text",
            BodySpec::UrlEncoded { .. } => "urlencoded",
            BodySpec::Multipart { .. } => "multipart",
            BodySpec::GraphQL { .. } => "graphql",
            BodySpec::Binary { .. } => "binary",
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, BodySpec::None)
    }
}

/// 测试配置（pre_request / extract / assertions），存储为 JSON。
pub type TestConfig = serde_json::Value;

/// 统一请求结构。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestSpec {
    #[serde(default)]
    pub params: Vec<KeyValue>,
    #[serde(default)]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub path_variables: Vec<KeyValue>,
    #[serde(default)]
    pub auth: AuthSpec,
    #[serde(default)]
    pub body: BodySpec,
    /// 编辑器配置 Tab（params/auth/headers/body/...）：用户最近一次停留位置。
    /// 为空时前端按 HTTP Method 智能默认（POST 系 → body，其余 → params）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tab: Option<String>,
    /// 请求超时（毫秒）。`None` = 使用全局默认（设置中的请求超时）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default = "default_true")]
    pub follow_redirects: bool,
    #[serde(default)]
    pub tests: Option<TestConfig>,
    /// 禁用 Cookie 自动回放（默认 false = 携带 jar 中的同域 Cookie）。
    #[serde(default)]
    pub disable_cookies: bool,
}

impl Default for RequestSpec {
    fn default() -> Self {
        RequestSpec {
            params: Vec::new(),
            headers: Vec::new(),
            path_variables: Vec::new(),
            auth: AuthSpec::None,
            body: BodySpec::None,
            active_tab: None,
            timeout_ms: None,
            follow_redirects: true,
            tests: None,
            disable_cookies: false,
        }
    }
}

/// 项目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub variables: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 文件夹。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub id: Uuid,
    pub project_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub sort_order: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 接口。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Endpoint {
    pub id: Uuid,
    pub project_id: Uuid,
    pub folder_id: Option<Uuid>,
    pub name: String,
    pub method: HttpMethod,
    pub path: String,
    pub description: String,
    pub status: EndpointStatus,
    pub sort_order: i64,
    pub request: RequestSpec,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 模块 / 服务的前置 URL 配置（环境内一个可命中目标）。
///
/// 微服务场景下同一环境（如「测试环境」）内，支付 / 收单 / 渠道等服务各自
/// 拥有独立基址；请求未显式绑定模块时回退到 `is_default` 模块。
///
/// 全局环境下模块与项目联动：`project_id` 为 `Some` 的模块对应一个项目
/// （module_name 随项目名自动同步），`None` 为手工维护的临时模块。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleUrlConfig {
    pub id: Uuid,
    /// 关联的项目（模块自动同步项目时绑定；临时模块为 `None`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// 模块名（项目模块随项目名同步；临时模块可手工命名）。
    pub module_name: String,
    /// 前置 URL，如 `http://dev-test01.redotpay.inet:8092`（可包含 `{{变量}}`）。
    pub base_url: String,
    /// 是否为默认模块（未显式指定模块时使用）。
    #[serde(default)]
    pub is_default: bool,
}

/// 环境变量（支持远程 / 本地双值：本地值优先覆盖远程值）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentVariable {
    pub key: String,
    /// 远程 / 公共值。
    pub remote_value: String,
    /// 本地私有覆盖值（非空时优先于 remote_value）。
    #[serde(default)]
    pub local_value: String,
    /// 是否参与注入（关闭后该变量不进入请求变量表）。
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// 全局参数注入位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GlobalParamLocation {
    /// 拼入每个请求的 URL 查询参数。
    #[default]
    Query,
    /// 注入每个请求的请求头。
    Header,
}

/// 全局参数（注入制）：每个请求自动附加的 key/value。
///
/// 与「全局变量」的区别：
/// - 全局变量是「引用制」——请求里写 `{{name}}` 才按名替换；
/// - 全局参数是「注入制」——定义后每个请求自动并入 query / header，无需手动写。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalParam {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub location: GlobalParamLocation,
}

/// 环境（全局维度）。
///
/// 环境跨项目共享；每个环境按「服务 / 模块」维护各自的 Base URL `modules`
/// （项目模块自动同步全部项目），同时持有一组结构化环境变量 `variables`
/// （本地值覆盖远程值）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub id: Uuid,
    pub name: String,
    /// 多模块前置 URL 列表。
    #[serde(default)]
    pub modules: Vec<ModuleUrlConfig>,
    /// 结构化环境变量列表。
    #[serde(default)]
    pub variables: Vec<EnvironmentVariable>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for ModuleUrlConfig {
    fn default() -> Self {
        ModuleUrlConfig {
            id: Uuid::new_v4(),
            project_id: None,
            module_name: String::new(),
            base_url: String::new(),
            is_default: false,
        }
    }
}

impl EnvironmentVariable {
    /// 生效值：本地覆盖值非空时取本地，否则取远程 / 公共值。
    pub fn effective_value(&self) -> &str {
        let local = self.local_value.trim();
        if !local.is_empty() {
            local
        } else {
            self.remote_value.trim()
        }
    }

    pub fn is_effective(&self) -> bool {
        self.enabled && !self.effective_value().is_empty()
    }
}

impl Environment {
    /// 默认模块：优先**当前项目绑定的模块**（project_id 匹配），其次 `is_default`，
    /// 否则取第一个（兼容无标记模块的旧数据）。
    ///
    /// 项目偏好让多项目共用一个环境时，「默认模块」随所在项目自动落在该
    /// 项目自己的基址上，而不是全局钉死的 is_default 模块。
    pub fn default_module(&self, project_id: Option<Uuid>) -> Option<&ModuleUrlConfig> {
        if let Some(pid) = project_id {
            if let Some(m) = self.modules.iter().find(|m| m.project_id == Some(pid)) {
                return Some(m);
            }
        }
        self.modules
            .iter()
            .find(|m| m.is_default)
            .or_else(|| self.modules.first())
    }

    /// 按模块 id 或模块名查找模块。
    pub fn module(&self, key: &str) -> Option<&ModuleUrlConfig> {
        let n = key.trim();
        if n.is_empty() {
            return self.default_module(None);
        }
        self.modules
            .iter()
            .find(|m| m.id.to_string() == n || m.module_name == n)
    }

    /// 生效环境变量扁平表（仅 enabled；本地值优先）。
    pub fn effective_variables(&self) -> HashMap<String, String> {
        self.variables
            .iter()
            .filter(|v| v.is_effective())
            .map(|v| (v.key.trim().to_string(), v.effective_value().to_string()))
            .collect()
    }

    /// 当前环境实际采用的基址：显式模块 > 默认模块；无可用模块返回 `None`。
    ///
    /// `module_key` 为 `None` 或空时表示「未指定模块 → 默认模块」。
    pub fn base_url(&self, module_key: Option<&str>, project_id: Option<Uuid>) -> Option<&str> {
        let module = match module_key {
            Some(key) => self.module(key)?,
            None => self.default_module(project_id)?,
        };
        let base = module.base_url.trim();
        if base.is_empty() {
            None
        } else {
            Some(base)
        }
    }
}

/// 响应示例。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseExample {
    pub id: Uuid,
    pub endpoint_id: Uuid,
    pub name: String,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub content_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 请求用例（请求快照）。
///
/// 保存接口某次请求的完整 `RequestSpec`（参数 / 认证 / 请求头 / Body / 超时等），
/// 可一键回填编辑器复用，避免反复手工拼装。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestExample {
    pub id: Uuid,
    pub endpoint_id: Uuid,
    pub name: String,
    pub request: RequestSpec,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 测试用例运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum TestCaseStatus {
    Success,
    Failed,
    Untested,
}

impl TestCaseStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TestCaseStatus::Success => "Success",
            TestCaseStatus::Failed => "Failed",
            TestCaseStatus::Untested => "Untested",
        }
    }

    pub fn parse(s: &str) -> Option<TestCaseStatus> {
        match s {
            "Success" => Some(TestCaseStatus::Success),
            "Failed" => Some(TestCaseStatus::Failed),
            "Untested" => Some(TestCaseStatus::Untested),
            _ => None,
        }
    }
}

/// 测试用例（Apifox 风格用例管理）。
///
/// 每个用例保存一次调用的独立快照：URL 路径 + Query 参数 + 请求头 + Body，
/// 可单独运行、一键回填调试页重新调用。`request_id` 关联主接口（`endpoints`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestCase {
    pub id: Uuid,
    /// 关联的主接口 ID（endpoints.id）。
    pub request_id: Uuid,
    pub name: String,
    /// 用例分组：正向 / 负向 / 边界值 / 安全性 / 其他。
    pub category: String,
    pub method: HttpMethod,
    pub url_path: String,
    pub params: Vec<KeyValue>,
    pub headers: Vec<KeyValue>,
    /// body 类型标识：json / form-data / raw / urlencoded / graphql / binary / none。
    pub body_type: String,
    pub body_content: String,
    pub last_run_status: TestCaseStatus,
    pub created_at: DateTime<Utc>,
}

/// Mock 匹配条件项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockMatchItem {
    pub key: String,
    pub value: String,
}

/// 自定义 Mock 规则。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MockRule {
    pub id: Uuid,
    pub project_id: Uuid,
    pub endpoint_id: Option<Uuid>,
    pub name: String,
    pub method: HttpMethod,
    pub path: String,
    pub match_query: Vec<MockMatchItem>,
    pub match_headers: Vec<MockMatchItem>,
    pub response_status: u16,
    pub response_headers: HashMap<String, String>,
    pub response_body_template: String,
    pub delay_ms: u64,
    /// 故障注入：百分之多少的命中请求返回 fault_status（0 = 关闭）。
    #[serde(default)]
    pub fault_rate_pct: u8,
    /// 故障注入时的状态码（默认 500）。
    #[serde(default = "default_fault_status")]
    pub fault_status: u16,
    pub enabled: bool,
    pub priority: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 单次测试运行结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestRun {
    pub id: Uuid,
    pub project_id: Uuid,
    pub environment_id: Option<Uuid>,
    pub name: String,
    pub result_json: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// 请求历史。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestHistory {
    pub id: Uuid,
    pub project_id: Uuid,
    pub endpoint_id: Option<Uuid>,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub request_summary_json: String,
    pub response_summary_json: String,
    pub created_at: DateTime<Utc>,
}

/// 自增序列（`{{$seq:key}}`）；`value` 为下一次输出值。key 为空字符串表示全局 `{{$seq}}`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeqCounter {
    pub key: String,
    pub value: u64,
}

/// WebSocket 待发消息类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WsMessageType {
    Text,
    Binary,
    Ping,
}

impl WsMessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            WsMessageType::Text => "text",
            WsMessageType::Binary => "binary",
            WsMessageType::Ping => "ping",
        }
    }
}

/// 持久化的 WebSocket 待发消息（断线 / 队列溢出时落库）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WsMessageRecord {
    pub id: Uuid,
    pub message_type: WsMessageType,
    /// Text 为原文；Binary / Ping 为 base64 编码。
    pub payload: String,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_method_roundtrip() {
        for m in HttpMethod::all() {
            let s = serde_json::to_string(m).unwrap();
            assert_eq!(s, format!("\"{}\"", m.as_str()));
            let back: HttpMethod = serde_json::from_str(&s).unwrap();
            assert_eq!(back, *m);
            let parsed: HttpMethod = m.as_str().parse().unwrap();
            assert_eq!(parsed, *m);
        }
        assert!("TRACE".parse::<HttpMethod>().is_err());
    }

    #[test]
    fn status_serde() {
        let s = serde_json::to_string(&EndpointStatus::Developing).unwrap();
        assert_eq!(s, "\"developing\"");
        let back: EndpointStatus = serde_json::from_str("\"released\"").unwrap();
        assert_eq!(back, EndpointStatus::Released);
    }

    #[test]
    fn request_spec_default_json_shape() {
        let spec = RequestSpec::default();
        let json = serde_json::to_value(&spec).unwrap();
        assert_eq!(json["params"], serde_json::json!([]));
        assert_eq!(json["auth"]["type"], "none");
        assert_eq!(json["body"]["mode"], "none");
        assert_eq!(json["timeout_ms"], serde_json::Value::Null);
        assert_eq!(json["follow_redirects"], true);
    }

    #[test]
    fn keyvalue_design_metadata_roundtrip() {
        let kv = KeyValue {
            key: "userId".into(),
            value: "1".into(),
            enabled: true,
            description: "用户 ID".into(),
            field_type: FieldType::Number,
            required: false,
            example: "42".into(),
        };
        let json = serde_json::to_value(&kv).unwrap();
        assert_eq!(json["field_type"], "number");
        assert_eq!(json["required"], false);
        assert_eq!(json["example"], "42");
        let back: KeyValue = serde_json::from_value(json).unwrap();
        assert_eq!(back, kv);
    }

    /// 旧数据兼容：缺省 field_type / required / example 时按默认值反序列化。
    #[test]
    fn keyvalue_legacy_json_defaults() {
        let legacy = serde_json::json!({
            "key": "page", "value": "1", "enabled": true, "description": ""
        });
        let kv: KeyValue = serde_json::from_value(legacy).unwrap();
        assert_eq!(kv.field_type, FieldType::String);
        assert!(kv.required);
        assert_eq!(kv.example, "");
        // 新字段序列化后仍可被旧读取方忽略（未知字段不报错由 serde 默认保证）。
        let full = serde_json::to_string(&KeyValue::new("a", "1")).unwrap();
        assert!(full.contains("\"field_type\":\"string\""));
        assert!(full.contains("\"required\":true"));
    }

    #[test]
    fn body_spec_binary_json_shape() {
        let body = BodySpec::Binary {
            path: "/tmp/a.png".into(),
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["mode"], "binary");
        assert_eq!(json["path"], "/tmp/a.png");
        let back: BodySpec = serde_json::from_value(json).unwrap();
        assert_eq!(back, body);
    }

    #[test]
    fn auth_bearer_json_shape() {
        let auth = AuthSpec::Bearer {
            token: "{{token}}".into(),
        };
        let json = serde_json::to_value(&auth).unwrap();
        assert_eq!(json["type"], "bearer");
        assert_eq!(json["token"], "{{token}}");
        let back: AuthSpec = serde_json::from_value(json).unwrap();
        assert_eq!(back, auth);
    }

    #[test]
    fn auth_apikey_json_shape() {
        let auth = AuthSpec::ApiKey {
            key: "X-API-KEY".into(),
            value: "{{api_key}}".into(),
            location: ApiKeyLocation::Header,
        };
        let json = serde_json::to_value(&auth).unwrap();
        assert_eq!(json["type"], "apikey");
        assert_eq!(json["in"], "header");
        let back: AuthSpec = serde_json::from_value(json).unwrap();
        assert_eq!(back, auth);
    }

    #[test]
    fn auth_signing_types_json_shape_and_roundtrip() {
        // tag 命名与前端 `AuthSpec` 联合类型严格对应。
        let cases: Vec<(AuthSpec, &str)> = vec![
            (
                AuthSpec::Digest {
                    username: "u".into(),
                    password: "p".into(),
                },
                "digest",
            ),
            (
                AuthSpec::Hawk {
                    key_id: "id-1".into(),
                    key: "k".into(),
                },
                "hawk",
            ),
            (
                AuthSpec::AwsV4 {
                    access_key: "AK".into(),
                    secret_key: "SK".into(),
                    region: "cn-north-1".into(),
                    service: "s3".into(),
                    session_token: None,
                },
                "awsv4",
            ),
            (
                AuthSpec::Hmac {
                    access_key: "ak".into(),
                    secret_key: "sk".into(),
                },
                "hmac",
            ),
        ];
        for (auth, tag) in cases {
            let json = serde_json::to_value(&auth).unwrap();
            assert_eq!(json["type"], tag);
            let back: AuthSpec = serde_json::from_value(json).unwrap();
            assert_eq!(back, auth);
        }
        // session_token 为空时不序列化（紧凑存储）。
        let json = serde_json::to_value(&AuthSpec::AwsV4 {
            access_key: String::new(),
            secret_key: String::new(),
            region: String::new(),
            service: String::new(),
            session_token: None,
        })
        .unwrap();
        assert!(json.get("session_token").is_none());
    }

    #[test]
    fn body_urlencoded_json_shape() {
        let body = BodySpec::UrlEncoded {
            fields: vec![KeyValue::new("a", "1")],
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["mode"], "urlencoded");
        assert_eq!(json["fields"][0]["key"], "a");
        let back: BodySpec = serde_json::from_value(json).unwrap();
        assert_eq!(back, body);
    }

    #[test]
    fn oauth2_json_shape_and_roundtrip() {
        let auth = AuthSpec::OAuth2 {
            client_id: "my-client".into(),
            client_secret: "s3cret".into(),
            auth_url: "https://idp.example.com/authorize".into(),
            token_url: "https://idp.example.com/token".into(),
            scope: "openid profile".into(),
            redirect_uri: "http://127.0.0.1:9090/callback".into(),
            token: Some(OAuth2Token {
                access_token: "at-1".into(),
                token_type: "Bearer".into(),
                refresh_token: Some("rt-1".into()),
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            }),
        };
        let json = serde_json::to_value(&auth).unwrap();
        assert_eq!(json["type"], "oauth2");
        assert_eq!(json["client_id"], "my-client");
        assert_eq!(json["token"]["access_token"], "at-1");
        let back: AuthSpec = serde_json::from_value(json).unwrap();
        assert_eq!(back, auth);
    }

    #[test]
    fn oauth2_without_token_serializes_compact() {
        let auth = AuthSpec::OAuth2 {
            client_id: "c".into(),
            client_secret: "s".into(),
            auth_url: String::new(),
            token_url: String::new(),
            scope: String::new(),
            redirect_uri: String::new(),
            token: None,
        };
        let json = serde_json::to_value(&auth).unwrap();
        assert!(json.get("token").is_none());
    }

    #[test]
    fn oauth2_token_status_transitions() {
        let make = |hours: i64| AuthSpec::OAuth2 {
            client_id: String::new(),
            client_secret: String::new(),
            auth_url: String::new(),
            token_url: String::new(),
            scope: String::new(),
            redirect_uri: String::new(),
            token: Some(OAuth2Token {
                access_token: String::new(),
                token_type: String::new(),
                refresh_token: None,
                expires_at: chrono::Utc::now() + chrono::Duration::hours(hours),
            }),
        };
        assert_eq!(make(1).oauth2_status(), Some(OAuth2Status::Valid));
        // 30 秒后过期 → 即将过期
        let soon = AuthSpec::OAuth2 {
            client_id: String::new(),
            client_secret: String::new(),
            auth_url: String::new(),
            token_url: String::new(),
            scope: String::new(),
            redirect_uri: String::new(),
            token: Some(OAuth2Token {
                access_token: String::new(),
                token_type: String::new(),
                refresh_token: None,
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(30),
            }),
        };
        assert_eq!(soon.oauth2_status(), Some(OAuth2Status::ExpiringSoon));
        // 已过期
        let expired = AuthSpec::OAuth2 {
            client_id: String::new(),
            client_secret: String::new(),
            auth_url: String::new(),
            token_url: String::new(),
            scope: String::new(),
            redirect_uri: String::new(),
            token: Some(OAuth2Token {
                access_token: String::new(),
                token_type: String::new(),
                refresh_token: None,
                expires_at: chrono::Utc::now() - chrono::Duration::seconds(1),
            }),
        };
        assert_eq!(expired.oauth2_status(), Some(OAuth2Status::Expired));
        // 无 token
        let none = AuthSpec::OAuth2 {
            client_id: String::new(),
            client_secret: String::new(),
            auth_url: String::new(),
            token_url: String::new(),
            scope: String::new(),
            redirect_uri: String::new(),
            token: None,
        };
        assert_eq!(none.oauth2_status(), Some(OAuth2Status::Unauthorized));
        assert_eq!(AuthSpec::None.oauth2_status(), None);
    }
}
