use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::OnceLock;
use std::time::Duration;

use auth_base::{Identity, Role, RoleAuthorization, SessionVariableName, SessionVariableValue};
use axum::{
    http::{HeaderMap, HeaderName, StatusCode},
    response::IntoResponse,
};
use reqwest::{header::ToStrError, Url};
use serde::{de::Error as SerdeDeError, Deserialize, Deserializer, Serialize, Serializer};

use hasura_authn_core as auth_base;
use open_dds::{session_variables, EnvironmentValue};
use schemars::JsonSchema;
use serde_json::Value;
use tracing_util::{ErrorVisibility, SpanVisibility, TraceableError};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error in converting the header value corresponding to the {header_name} to a String - {error}")]
    ErrorInConvertingHeaderValueToString {
        header_name: HeaderName,
        error: ToStrError,
    },
    #[error("The Authentication hook has denied to execute the request.")]
    AuthenticationFailed,
    #[error("Internal Error - {0}")]
    Internal(#[from] InternalError),
}

impl TraceableError for Error {
    fn visibility(&self) -> ErrorVisibility {
        // For the purpose of traces, all webhook errors should be developer facing.
        ErrorVisibility::User
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InternalError {
    #[error("Error while making the authentication HTTP request to the webhook - {0}")]
    ErrorWhileMakingHTTPRequestToTheAuthHook(reqwest::Error),
    #[error("The authentication hook has returned the status {0}. Only 200 and 401 response status are recognized.")]
    AuthHookUnexpectedStatus(reqwest::StatusCode),
    #[error("Reqwest error: {0}")]
    ReqwestError(reqwest::Error),
    #[error("'x-hasura-role' session variable not found in the webhook response.")]
    RoleSessionVariableNotFound,
    #[error("'x-hasura-role' session variable in the webhook response was not a string.")]
    RoleSessionVariableMustBeString,
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

impl TraceableError for InternalError {
    fn visibility(&self) -> ErrorVisibility {
        // For the purpose of traces, all webhook errors should be developer facing.
        ErrorVisibility::User
    }
}

impl Error {
    pub fn to_status_code(&self) -> StatusCode {
        match self {
            Error::ErrorInConvertingHeaderValueToString { .. } => StatusCode::BAD_REQUEST,
            Error::AuthenticationFailed => StatusCode::FORBIDDEN,
            Error::Internal(_e) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let is_internal = match &self {
            Error::ErrorInConvertingHeaderValueToString { .. } | Error::AuthenticationFailed => {
                false
            }
            Error::Internal(_e) => true,
        };
        lang_graphql::http::Response::error_message_with_status(
            self.to_status_code(),
            self.to_string(),
            is_internal,
        )
        .into_response()
    }
}

fn serialize_url<S>(url: &Url, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(url.as_str())
}

fn deserialize_url<'de, D>(deserializer: D) -> Result<Url, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;

    Url::parse(&buf).map_err(SerdeDeError::custom)
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
#[schemars(title = "AuthHookMethod")]
pub enum AuthHookMethod {
    Get,
    Post,
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[schemars(title = "AuthHookConfig")]
#[schemars(example = "AuthHookConfig::example")]
/// The configuration of the authentication webhook.
pub struct AuthHookConfig {
    #[serde(serialize_with = "serialize_url", deserialize_with = "deserialize_url")]
    /// The URL of the authentication webhook.
    pub url: Url,
    /// The HTTP method to be used to make the request to the auth hook.
    pub method: AuthHookMethod,
}

impl AuthHookConfig {
    fn example() -> Self {
        serde_json::from_str(
            r#"
            {
                "url": "http://auth_hook:3050/validate-request",
                "method": "Post"
            }
        "#,
        )
        .unwrap()
    }
}

/// The body of the POST request to be made to the Auth hook.
#[derive(Serialize, Debug)]
struct AuthHookPostRequestBody {
    headers: HashMap<String, String>,
}

impl AuthHookPostRequestBody {
    fn new() -> Self {
        AuthHookPostRequestBody {
            headers: HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
#[schemars(title = "AuthHookConfigV3")]
#[schemars(example = "AuthHookConfigV3::example")]
#[serde(tag = "method")]
/// The configuration of the authentication webhook.
pub enum AuthHookConfigV3 {
    #[schemars(title = "AuthHookConfigV3GET")]
    /// The configuration of the GET authentication webhook.
    GET(AuthHookConfigV3GET),
    #[schemars(title = "AuthHookConfigV3POST")]
    /// The configuration of the POST authentication webhook.
    POST(AuthHookConfigV3POST),
}

impl AuthHookConfigV3 {
    fn example() -> Self {
        serde_json::from_str(
            r#"
            {
                "method": "GET",
                "url": {
                    "value": "http://auth_hook:3050/validate-request"
                },
                "customHeadersConfig": {
                    "headers": {
                        "forward": ["Authorization"],
                        "additional": {
                            "user-agent": "hasura-ddn"
                        }
                    }
                }
            }
        "#,
        )
        .unwrap()
    }
    pub fn get_url(&self) -> &str {
        match self {
            AuthHookConfigV3::GET(config) => &config.url.value,
            AuthHookConfigV3::POST(config) => &config.url.value,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[schemars(title = "AuthHookConfigV3GET")]
#[schemars(example = "AuthHookConfigV3GET::example")]
/// The configuration of the GET authentication webhook.
pub struct AuthHookConfigV3GET {
    /// The URL of the GET authentication webhook.
    pub url: EnvironmentValue,
    #[serde(default)]
    /// The configuration for the headers to be sent to the GET auth hook.
    pub custom_headers_config: Option<AuthHookConfigV3GETHeaders>,
}

impl AuthHookConfigV3GET {
    fn example() -> Self {
        serde_json::from_str(
            r#"
            {
                "url": {
                    "value": "http://auth_hook:3050/validate-request"
                },
                "customHeadersConfig": {
                    "headers": {
                        "forward": ["Authorization"],
                        "additional": {
                            "user-agent": "hasura-ddn"
                        }
                    }
                }
            }
        "#,
        )
        .unwrap()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[schemars(title = "AuthHookConfigV3GETHeaders")]
#[schemars(example = "AuthHookConfigV3GETHeaders::example")]
/// The configuration for the headers to be sent to the GET auth hook.
pub struct AuthHookConfigV3GETHeaders {
    #[serde(default)]
    headers: Option<AuthHookConfigV3Headers>,
}

impl AuthHookConfigV3GETHeaders {
    fn example() -> Self {
        serde_json::from_str(
            r#"
            {
                "headers": {
                    "forward": ["Authorization"],
                    "additional": {
                        "user-agent": "hasura-ddn"
                    }
                }
            }
        "#,
        )
        .unwrap()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[schemars(title = "AuthHookConfigV3POST")]
#[schemars(example = "AuthHookConfigV3POST::example")]
/// The configuration of the POST authentication webhook.
pub struct AuthHookConfigV3POST {
    /// The URL of the POST authentication webhook.
    pub url: EnvironmentValue,
    #[serde(default)]
    /// The configuration for the headers to be sent to the POST auth hook.
    pub custom_headers_config: Option<AuthHookConfigV3POSTHeaders>,
}

impl AuthHookConfigV3POST {
    fn example() -> Self {
        serde_json::from_str(
            r#"
            {
                "url": {
                    "value": "http://auth_hook:3050/validate-request"
                },
                "customHeadersConfig": {
                    "headers": {
                        "additional": {
                            "user-agent": "hasura-ddn"
                        }
                    },
                    "body": {
                        "headers": {
                            "forward": ["Authorization"]
                        }
                    }
                }
            }
        "#,
        )
        .unwrap()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[schemars(title = "AuthHookConfigV3POSTHeaders")]
#[schemars(example = "AuthHookConfigV3POSTHeaders::example")]
/// The configuration for the headers and body to be sent to the POST auth hook.
pub struct AuthHookConfigV3POSTHeaders {
    #[serde(default)]
    /// The configuration for the headers to be sent to the POST auth hook.
    headers: Option<AuthHookConfigV3Headers>,
    #[serde(default)]
    /// The configuration for the body to be sent to the POST auth hook.
    body: Option<AuthHookConfigV3Body>,
}

impl AuthHookConfigV3POSTHeaders {
    fn example() -> Self {
        serde_json::from_str(
            r#"
            {
                "headers": {
                    "additional": {
                        "user-agent": "hasura-ddn"
                    }
                },
                "body": {
                    "headers": {
                        "forward": ["Authorization"]
                    }
                }
            }
        "#,
        )
        .unwrap()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[schemars(title = "AuthHookConfigV3Headers")]
#[schemars(example = "AuthHookConfigV3Headers::example")]
/// The configuration for the headers to be sent to the auth hook.
pub struct AuthHookConfigV3Headers {
    #[serde(default = "serde_ext::ser_default::<AllOrList<String>>")]
    /// The headers to be forwarded from the client request.
    pub forward: AllOrList<String>,
    #[serde(default)]
    #[schemars(title = "AdditionalHeaders")]
    /// The additional headers to be sent to the auth hook.
    pub additional: HashMap<String, String>,
}

impl AuthHookConfigV3Headers {
    fn example() -> Self {
        serde_json::from_str(
            r#"
            {
                "forward": ["Authorization"],
                "additional": {
                    "user-agent": "hasura-ddn"
                }
            }
        "#,
        )
        .unwrap()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(untagged)]
#[schemars(title = "AllOrList")]
#[schemars(example = "AllOrList::<String>::example")]
/// A list of items or a wildcard.
pub enum AllOrList<T> {
    All(All),
    List(Vec<T>),
}

impl<T: PartialEq> serde_ext::HasDefaultForSerde for AllOrList<T> {
    fn ser_default() -> Self {
        AllOrList::All(All(()))
    }
}

impl<T> AllOrList<T>
where
    for<'de> T: Deserialize<'de>,
{
    fn example() -> Self {
        serde_json::from_str(r#""*""#).unwrap()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Wildcard: match all items
pub struct All(());

impl Serialize for All {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str("*")
    }
}

impl<'de> Deserialize<'de> for All {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::String(s) if s == "*" => Ok(All(())),
            _ => Err(SerdeDeError::custom("Invalid value for All, expected '*'")),
        }
    }
}

impl schemars::JsonSchema for All {
    fn schema_name() -> String {
        "All".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::Schema::Object(schemars::schema::SchemaObject {
            metadata: Some(Box::new(schemars::schema::Metadata {
                title: Some(Self::schema_name()),
                description: Some("Wildcard: match all items".to_owned()),
                ..Default::default()
            })),
            instance_type: Some(schemars::schema::SingleOrVec::Single(Box::new(
                schemars::schema::InstanceType::String,
            ))),
            enum_values: Some(vec![serde_json::Value::String("*".to_string())]),
            ..Default::default()
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[schemars(title = "AuthHookConfigV3Body")]
#[schemars(example = "AuthHookConfigV3Body::example")]
/// The configuration for the body to be sent to the POST auth hook.
pub struct AuthHookConfigV3Body {
    #[serde(default)]
    /// The configuration for the headers to be sent as part of the body to the POST auth hook.
    pub headers: Option<AuthHookConfigV3Headers>,
}

impl AuthHookConfigV3Body {
    fn example() -> Self {
        serde_json::from_str(
            r#"
            {
                "headers": {
                    "forward": ["Authorization"]
                }
            }
        "#,
        )
        .unwrap()
    }
}

async fn make_auth_hook_request(
    // HTTP client that needs to be passed to make the
    // HTTP request to the auth hook.
    http_client: &reqwest::Client,
    auth_hook_url: &Url,
    request: AuthHookRequest,
    allow_role_emulation_for: Option<&Role>,
) -> Result<auth_base::Identity, Error> {
    let tracer = tracing_util::global_tracer();
    let http_request_builder = match request {
        AuthHookRequest::Get { headers } => {
            let mut auth_hook_headers = tracing_util::get_trace_headers();
            auth_hook_headers.extend(headers);
            http_client
                .get(auth_hook_url.clone())
                .headers(auth_hook_headers)
                .timeout(Duration::from_secs(60))
        }
        AuthHookRequest::Post {
            headers,
            body: request_body,
        } => {
            let mut auth_hook_headers = tracing_util::get_trace_headers();
            auth_hook_headers.extend(headers);
            http_client
                .post(auth_hook_url.clone())
                .headers(auth_hook_headers)
                .json(&request_body)
                .timeout(Duration::from_secs(60))
        }
    };

    let req = http_request_builder
        .build()
        .map_err(InternalError::ReqwestError)?;

    let response = tracer
        .in_span_async(
            "request_to_webhook",
            "Send request to webhook",
            SpanVisibility::Internal,
            || {
                Box::pin(async {
                    http_client
                        .execute(req)
                        .await
                        .map_err(InternalError::ErrorWhileMakingHTTPRequestToTheAuthHook)
                })
            },
        )
        .await?;

    match response.status() {
        reqwest::StatusCode::UNAUTHORIZED => Err(Error::AuthenticationFailed),
        reqwest::StatusCode::OK => {
            let auth_hook_response: HashMap<String, serde_json::Value> =
                response.json().await.map_err(InternalError::ReqwestError)?;
            let mut session_variables = HashMap::new();
            for (k, v) in &auth_hook_response {
                match SessionVariableName::from_str(k) {
                    Ok(session_variable) => {
                        session_variables
                            .insert(session_variable, SessionVariableValue::Parsed(v.clone()));
                    }
                    Err(_e) => {}
                }
            }
            let role = auth_base::Role::new(
                session_variables
                    .get(&session_variables::SESSION_VARIABLE_ROLE)
                    .ok_or(InternalError::RoleSessionVariableNotFound)?
                    .as_str()
                    .ok_or_else(|| InternalError::RoleSessionVariableMustBeString)?,
            );
            let role_authorization = RoleAuthorization {
                role: role.clone(),
                session_variables,
                allowed_session_variables_from_request: auth_base::SessionVariableList::Some(
                    HashSet::new(),
                ),
            };
            let mut allowed_roles = HashMap::new();
            allowed_roles.insert(role.clone(), role_authorization);

            Ok(match allow_role_emulation_for {
                Some(emulation_role) => {
                    if role == *emulation_role {
                        Identity::RoleEmulationEnabled(role)
                    } else {
                        Identity::Specific {
                            default_role: role,
                            allowed_roles,
                        }
                    }
                }
                None => Identity::Specific {
                    default_role: role,
                    allowed_roles,
                },
            })
        }
        status_code => Err(InternalError::AuthHookUnexpectedStatus(status_code))?,
    }
}

/// Makes the HTTP request to the auth hook. The webhook
/// is authenticates the request based on the client headers.
pub async fn authenticate_request(
    // HTTP client that needs to be passed to make the
    // HTTP request to the auth hook.
    http_client: &reqwest::Client,
    auth_hook_config: &AuthHookConfig,
    client_headers: &HeaderMap,
    allow_role_emulation_for: Option<&Role>,
) -> Result<auth_base::Identity, Error> {
    let tracer = tracing_util::global_tracer();
    tracer
        .in_span_async(
            "webhook_authenticate_request",
            "Webhook authenticate request",
            SpanVisibility::Internal,
            || {
                Box::pin({
                    let headers =
                        get_auth_hook_request_v1(&auth_hook_config.method, client_headers);
                    make_auth_hook_request(
                        http_client,
                        &auth_hook_config.url,
                        headers,
                        allow_role_emulation_for,
                    )
                })
            },
        )
        .await
}

/// Makes the HTTP request to the auth hook. The webhook
/// is authenticates the request based on the client headers.
pub async fn authenticate_request_v2(
    // HTTP client that needs to be passed to make the
    // HTTP request to the auth hook.
    http_client: &reqwest::Client,
    auth_hook_config: &AuthHookConfigV3,
    client_headers: &HeaderMap,
    allow_role_emulation_for: Option<&Role>,
) -> Result<auth_base::Identity, Error> {
    let tracer = tracing_util::global_tracer();
    tracer
        .in_span_async(
            "webhook_authenticate_request",
            "Webhook authenticate request",
            SpanVisibility::Internal,
            || {
                Box::pin(async {
                    let request = get_auth_hook_request_v2(auth_hook_config, client_headers);
                    let url_str = auth_hook_config.get_url();
                    match Url::from_str(url_str) {
                        Ok(url) => {
                            make_auth_hook_request(
                                http_client,
                                &url,
                                request,
                                allow_role_emulation_for,
                            )
                            .await
                        }
                        // This should never happen as the URL is validated beforehand.
                        Err(_) => Err(Error::Internal(InternalError::InvalidUrl(
                            url_str.to_string(),
                        ))),
                    }
                })
            },
        )
        .await
}

/// Ignore the following list of request headers, sent by the client when making a GET request to
/// the auth hook.
///
/// Note that, in the case the auth hook mode is `POST`, this is *not* applicable, i.e. all the
/// headers sent by the client are forwarded to the auth hook.
const COMMON_CLIENT_HEADERS_TO_IGNORE: [&str; 14] = [
    "Accept",
    "Accept-Datetime",
    "Accept-Encoding",
    "Accept-Language",
    "Cache-Control",
    "Connection",
    "Content-Length",
    "Content-MD5",
    "Content-Type",
    "DNT",
    "Host",
    "Origin",
    "Referer",
    "User-Agent",
];

/// Decides whether to ignore the given header sent by the client when making a GET request to
/// the auth hook.
///
/// The header is compared to a static list of headers (above).
///
/// Note that, in the case the auth hook mode is `POST`, this is *not* applicable, i.e. all the
/// headers sent by the client are forwarded to the auth hook.
fn ignore_header(header: &str) -> bool {
    static CELL: OnceLock<HashSet<String>> = OnceLock::new();
    CELL.get_or_init(|| HashSet::from(COMMON_CLIENT_HEADERS_TO_IGNORE.map(str::to_lowercase)))
        .contains(header)
}

enum AuthHookRequest {
    Get {
        headers: HeaderMap,
    },
    Post {
        headers: HeaderMap,
        body: AuthHookPostRequestBody,
    },
}

/// This is the older version of the auth hook headers. For GET requests, the headers are filtered
/// and only the headers that are not in the `COMMON_CLIENT_HEADERS_TO_IGNORE` list are forwarded to
/// the auth hook.
///
/// For POST requests, all the headers are forwarded to the auth hook but as request body.
fn get_auth_hook_request_v1(
    auth_hook_method: &AuthHookMethod,
    client_headers: &HeaderMap,
) -> AuthHookRequest {
    match auth_hook_method {
        AuthHookMethod::Get => {
            let mut auth_hook_headers = HeaderMap::new();
            for (header_name, header_value) in client_headers {
                if !ignore_header(header_name.as_str()) {
                    auth_hook_headers.insert(header_name, header_value.clone());
                }
            }
            AuthHookRequest::Get {
                headers: auth_hook_headers,
            }
        }
        AuthHookMethod::Post => {
            let mut request_body_headers = HashMap::new();
            for (header_name, header_value) in client_headers {
                request_body_headers.insert(
                    header_name.to_string(),
                    header_value.to_str().unwrap_or("").to_string(),
                );
            }
            AuthHookRequest::Post {
                // For POST requests (in config v1), we don't forward any headers to the auth hook.
                headers: HeaderMap::new(),
                body: AuthHookPostRequestBody {
                    headers: request_body_headers,
                },
            }
        }
    }
}

/// This is the newer version of the auth hook headers config. The headers are filtered and only the headers that are in
/// the `forward` list are forwarded to the auth hook. We also add the additional headers to the auth hook headers.
fn get_filtered_headers(
    auth_hook_headers_config: &AuthHookConfigV3Headers,
    client_headers: &HeaderMap,
    ignore_common_headers: bool,
) -> HeaderMap {
    let mut auth_hook_headers = HeaderMap::new();
    match &auth_hook_headers_config.forward {
        AllOrList::All(_) => {
            for (header_name, header_value) in client_headers {
                if ignore_common_headers && ignore_header(header_name.as_str()) {
                    continue;
                }
                auth_hook_headers.insert(header_name, header_value.clone());
            }
        }
        AllOrList::List(list) => {
            for (header_name, header_value) in client_headers {
                if list.contains(&header_name.to_string()) {
                    auth_hook_headers.insert(header_name, header_value.clone());
                }
            }
        }
    }
    for (header_name, header_value) in &auth_hook_headers_config.additional {
        auth_hook_headers.insert(
            HeaderName::from_str(header_name.as_str()).unwrap(),
            header_value.parse().unwrap(),
        );
    }
    auth_hook_headers
}

/// This is the newer version of the auth hook config. For GET requests, the headers are filtered based on the config.
/// For POST requests, the headers are filtered based on the config and the body is constructed based on the config for
/// the body.
fn get_auth_hook_request_v2(
    auth_hook_config: &AuthHookConfigV3,
    client_headers: &HeaderMap,
) -> AuthHookRequest {
    match auth_hook_config {
        AuthHookConfigV3::GET(config) => {
            match &config.custom_headers_config {
                Some(config) => {
                    let headers = match &config.headers {
                        Some(headers) => get_filtered_headers(headers, client_headers, true),
                        // If the headers config is not present, we don't forward any headers to the auth hook.
                        None => HeaderMap::new(),
                    };
                    AuthHookRequest::Get { headers }
                }
                // If the custom headers config is not present, we fallback to the older version of the auth hook config.
                None => get_auth_hook_request_v1(&AuthHookMethod::Get, client_headers),
            }
        }
        AuthHookConfigV3::POST(config) => match &config.custom_headers_config {
            Some(config) => {
                let headers = match &config.headers {
                    Some(headers) => get_filtered_headers(headers, client_headers, true),
                    // If the headers config is not present, we don't forward any headers to the auth hook.
                    None => HeaderMap::new(),
                };
                let body = match &config.body {
                    Some(body) => {
                        let headers_map = match &body.headers {
                            // We don't ignore the common headers here as the headers are sent as part of the body.
                            Some(headers) => get_filtered_headers(headers, client_headers, false),
                            None => HeaderMap::new(),
                        };
                        let mut headers = HashMap::new();
                        for (header_name, header_value) in &headers_map {
                            headers.insert(
                                header_name.to_string(),
                                header_value.to_str().unwrap_or("").to_string(),
                            );
                        }
                        AuthHookPostRequestBody { headers }
                    }
                    // If the body config is not present, we send an empty body.
                    None => AuthHookPostRequestBody::new(),
                };
                AuthHookRequest::Post { headers, body }
            }
            // If the custom headers config is not present, we fallback to the older version of the auth hook config.
            None => get_auth_hook_request_v1(&AuthHookMethod::Post, client_headers),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use auth_base::Role;
    use mockito;
    use rand::{thread_rng, Rng};
    use reqwest::header::CONTENT_TYPE;
    use serde_json::json;

    #[tokio::test]
    // This test emulates a successful authentication by the webhook using Get method
    async fn test_get_successful_webhook_auth() {
        // Request a new server from the pool
        let mut server = mockito::Server::new_async().await;

        let url = server.url();

        // Create a mock
        let mock = server
            .mock("GET", "/validate-request")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("x-api-key", "1234")
            .with_body("{\"x-hasura-role\": \"test-role\", \"x-hasura-test-role-id\": \"1\"}")
            .match_header("foo", "baz") // Make sure, other than the common client headers, other headers are forwarded correctly.
            .create();

        let http_client = reqwest::Client::new();

        let webhook_url = url + "/validate-request";

        let auth_hook_config_str =
            format!("{{ \"url\": \"{webhook_url}\", \"method\": \"Get\"  }}");

        let auth_hook_config: AuthHookConfig = serde_json::from_str(&auth_hook_config_str).unwrap();

        let mut client_headers = HeaderMap::new();
        client_headers.insert("foo", "baz".parse().unwrap());

        let request = get_auth_hook_request_v1(&auth_hook_config.method, &client_headers);

        let auth_response =
            make_auth_hook_request(&http_client, &auth_hook_config.url, request, None)
                .await
                .unwrap();

        mock.assert(); // Make sure the webhook has been called.

        let test_role = Role::new("test-role");
        let mut expected_allowed_roles = HashMap::new();
        let mut role_authorization_session_variables = HashMap::new();
        role_authorization_session_variables.insert(
            SessionVariableName::from_str("x-hasura-role").unwrap(),
            SessionVariableValue::Parsed(json!("test-role")),
        );
        role_authorization_session_variables.insert(
            SessionVariableName::from_str("x-hasura-test-role-id").unwrap(),
            SessionVariableValue::Parsed(json!("1")),
        );
        expected_allowed_roles.insert(
            test_role.clone(),
            RoleAuthorization {
                role: test_role,
                session_variables: role_authorization_session_variables,
                allowed_session_variables_from_request: auth_base::SessionVariableList::Some(
                    HashSet::new(),
                ),
            },
        );
        assert_eq!(
            auth_response,
            Identity::Specific {
                default_role: Role::new("test-role"),
                allowed_roles: expected_allowed_roles
            }
        );
    }

    #[tokio::test]
    // This test emulates a successful authentication by the webhook using Post method
    async fn test_post_webhook_successful_post_auth() {
        // Request a new server from the pool
        let mut server = mockito::Server::new_async().await;

        let url = server.url();

        // Create a mock
        let mock = server
            .mock("POST", "/validate-request")
            .match_body(r#"{"headers":{"foo":"baz"}}"#)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                      "x-hasura-role": "test-role",
                      "x-hasura-test-role-id": "1"
                   }"#,
            )
            .create();

        let http_client = reqwest::Client::new();

        let webhook_url = url + "/validate-request";

        let auth_hook_config_str =
            format!("{{ \"url\": \"{webhook_url}\", \"method\": \"Post\"  }}");

        let auth_hook_config: AuthHookConfig = serde_json::from_str(&auth_hook_config_str).unwrap();

        let mut client_headers = HeaderMap::new();
        client_headers.insert("foo", "baz".parse().unwrap());

        let request = get_auth_hook_request_v1(&auth_hook_config.method, &client_headers);

        let auth_response =
            make_auth_hook_request(&http_client, &auth_hook_config.url, request, None)
                .await
                .unwrap();

        mock.assert(); // Make sure the webhook has been called.

        let test_role = Role::new("test-role");
        let mut expected_allowed_roles = HashMap::new();
        let mut role_authorization_session_variables = HashMap::new();
        role_authorization_session_variables.insert(
            SessionVariableName::from_str("x-hasura-role").unwrap(),
            SessionVariableValue::Parsed(json!("test-role")),
        );
        role_authorization_session_variables.insert(
            SessionVariableName::from_str("x-hasura-test-role-id").unwrap(),
            SessionVariableValue::Parsed(json!("1")),
        );
        expected_allowed_roles.insert(
            test_role.clone(),
            RoleAuthorization {
                role: test_role,
                session_variables: role_authorization_session_variables,
                allowed_session_variables_from_request: auth_base::SessionVariableList::Some(
                    HashSet::new(),
                ),
            },
        );
        assert_eq!(
            auth_response,
            Identity::Specific {
                default_role: Role::new("test-role"),
                allowed_roles: expected_allowed_roles
            }
        );
    }

    #[tokio::test]
    // This test emulates the scenario where the webhook may return non-hasura session variables
    async fn test_allow_non_hasura_session_variables_in_webhook_response() {
        // Request a new server from the pool
        let mut server = mockito::Server::new_async().await;

        let url = server.url();

        // Create a mock
        let mock = server
            .mock("POST", "/validate-request")
            .match_body(r#"{"headers":{"foo":"baz"}}"#)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                      "x-hasura-role": "test-role",
                      "x-hasura-test-role-id": "1",
                      "status": "true"
                   }"#,
            )
            .create();

        let http_client = reqwest::Client::new();

        let webhook_url = url + "/validate-request";

        let auth_hook_config_str =
            format!("{{ \"url\": \"{webhook_url}\", \"method\": \"Post\"  }}");

        let auth_hook_config: AuthHookConfig = serde_json::from_str(&auth_hook_config_str).unwrap();

        let mut client_headers = HeaderMap::new();
        client_headers.insert("foo", "baz".parse().unwrap());

        let request = get_auth_hook_request_v1(&auth_hook_config.method, &client_headers);

        let auth_response =
            make_auth_hook_request(&http_client, &auth_hook_config.url, request, None)
                .await
                .unwrap();

        mock.assert(); // Make sure the webhook has been called.

        let test_role = Role::new("test-role");
        let mut expected_allowed_roles = HashMap::new();
        let mut role_authorization_session_variables = HashMap::new();
        role_authorization_session_variables.insert(
            SessionVariableName::from_str("x-hasura-role").unwrap(),
            SessionVariableValue::Parsed(json!("test-role")),
        );
        role_authorization_session_variables.insert(
            SessionVariableName::from_str("x-hasura-test-role-id").unwrap(),
            SessionVariableValue::Parsed(json!("1")),
        );
        role_authorization_session_variables.insert(
            SessionVariableName::from_str("status").unwrap(),
            SessionVariableValue::Parsed(json!("true")),
        );
        expected_allowed_roles.insert(
            test_role.clone(),
            RoleAuthorization {
                role: test_role,
                session_variables: role_authorization_session_variables,
                allowed_session_variables_from_request: auth_base::SessionVariableList::Some(
                    HashSet::new(),
                ),
            },
        );
        assert_eq!(
            auth_response,
            Identity::Specific {
                default_role: Role::new("test-role"),
                allowed_roles: expected_allowed_roles
            }
        );
    }

    #[tokio::test]
    // This test emulates the scenario where role emulation is allowed
    async fn test_role_emulation() {
        // Request a new server from the pool
        let mut server = mockito::Server::new_async().await;

        let url = server.url();

        // Create a mock
        let mock = server
            .mock("POST", "/validate-request")
            .match_body(r#"{"headers":{"foo":"baz"}}"#)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                      "x-hasura-role": "test-admin-role",
                      "x-hasura-user-id": "1"
                   }"#,
            )
            .create();

        let http_client = reqwest::Client::new();

        let webhook_url = url + "/validate-request";

        let auth_hook_config_str =
            format!("{{ \"url\": \"{webhook_url}\", \"method\": \"Post\"  }}");

        let auth_hook_config: AuthHookConfig = serde_json::from_str(&auth_hook_config_str).unwrap();

        let mut client_headers = HeaderMap::new();
        client_headers.insert("foo", "baz".parse().unwrap());

        let request = get_auth_hook_request_v1(&auth_hook_config.method, &client_headers);

        let auth_response = make_auth_hook_request(
            &http_client,
            &auth_hook_config.url,
            request,
            Some(&Role::new("test-admin-role")),
        )
        .await
        .unwrap();

        mock.assert(); // Make sure the webhook has been called.

        let test_role = Role::new("test-admin-role");
        assert_eq!(auth_response, Identity::RoleEmulationEnabled(test_role));
    }

    #[tokio::test]
    // This test emulates the scenario where role emulation is denied
    async fn test_deny_role_emulation() {
        // Request a new server from the pool
        let mut server = mockito::Server::new_async().await;

        let url = server.url();

        // Create a mock
        let mock = server
            .mock("POST", "/validate-request")
            .match_body(r#"{"headers":{"foo":"baz"}}"#)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                      "x-hasura-role": "user",
                      "x-hasura-user-id": "1"
                   }"#,
            )
            .create();

        let http_client = reqwest::Client::new();

        let webhook_url = url + "/validate-request";

        let auth_hook_config_str =
            format!("{{ \"url\": \"{webhook_url}\", \"method\": \"Post\"  }}");

        let auth_hook_config: AuthHookConfig = serde_json::from_str(&auth_hook_config_str).unwrap();

        let mut client_headers = HeaderMap::new();
        client_headers.insert("foo", "baz".parse().unwrap());

        let request = get_auth_hook_request_v1(&auth_hook_config.method, &client_headers);

        let auth_response = make_auth_hook_request(
            &http_client,
            &auth_hook_config.url,
            request,
            Some(&Role::new("test-admin-role")),
        )
        .await
        .unwrap();

        mock.assert(); // Make sure the webhook has been called.

        let test_role = Role::new("user");
        let mut expected_allowed_roles = HashMap::new();
        let mut role_authorization_session_variables = HashMap::new();
        role_authorization_session_variables.insert(
            SessionVariableName::from_str("x-hasura-role").unwrap(),
            SessionVariableValue::Parsed(json!("user")),
        );
        role_authorization_session_variables.insert(
            SessionVariableName::from_str("x-hasura-user-id").unwrap(),
            SessionVariableValue::Parsed(json!("1")),
        );
        expected_allowed_roles.insert(
            test_role.clone(),
            RoleAuthorization {
                role: test_role.clone(),
                session_variables: role_authorization_session_variables,
                allowed_session_variables_from_request: auth_base::SessionVariableList::Some(
                    HashSet::new(),
                ),
            },
        );
        assert_eq!(
            auth_response,
            Identity::Specific {
                default_role: test_role,
                allowed_roles: expected_allowed_roles
            }
        );
    }

    #[tokio::test]
    /// This test emulates denial of the  authentication
    /// by sending a 401 status
    async fn test_post_webhook_denies_authentication() {
        // Request a new server from the pool
        let mut server = mockito::Server::new_async().await;

        let url = server.url();

        // Create a mock
        let mock = server
            .mock("POST", "/validate-request")
            .with_status(401)
            .with_header("content-type", "application/json")
            .create();

        let http_client = reqwest::Client::new();

        let webhook_url = url + "/validate-request";

        let auth_hook_config_str =
            format!("{{ \"url\": \"{webhook_url}\", \"method\": \"Post\"  }}");

        let auth_hook_config: AuthHookConfig = serde_json::from_str(&auth_hook_config_str).unwrap();

        let mut client_headers = HeaderMap::new();
        client_headers.insert("foo", "baz".parse().unwrap());
        client_headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());

        let request = get_auth_hook_request_v1(&auth_hook_config.method, &client_headers);

        let auth_response =
            make_auth_hook_request(&http_client, &auth_hook_config.url, request, None).await;

        mock.assert(); // Make sure the webhook has been called.

        assert_eq!(
            auth_response.unwrap_err().to_string(),
            "The Authentication hook has denied to execute the request."
        );
    }

    #[tokio::test]
    /// Test HTTP status codes returned by the webhook,
    /// other than 200 and 401 are not recognized.
    async fn test_webhook_returning_arbitrary_status() {
        // Request a new server from the pool
        let mut server = mockito::Server::new_async().await;

        let url = server.url();

        let mut rng = thread_rng();

        // Generate a random HTTP status code
        let mut http_status_code: usize = rng.gen_range(100..600);

        // Make sure that it's not either 200/401.
        while http_status_code == 200 || http_status_code == 401 {
            http_status_code = rng.gen_range(100..600);
        }

        // Create a mock
        let mock = server
            .mock("POST", "/validate-request")
            .with_status(http_status_code)
            .with_header("content-type", "application/json")
            .create();

        let http_client = reqwest::Client::new();

        let webhook_url = url + "/validate-request";

        let auth_hook_config_str =
            format!("{{ \"url\": \"{webhook_url}\", \"method\": \"Post\"  }}");

        let auth_hook_config: AuthHookConfig = serde_json::from_str(&auth_hook_config_str).unwrap();

        let mut client_headers = HeaderMap::new();
        client_headers.insert("foo", "baz".parse().unwrap());
        client_headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());

        let request = get_auth_hook_request_v1(&auth_hook_config.method, &client_headers);

        let auth_response =
            make_auth_hook_request(&http_client, &auth_hook_config.url, request, None).await;

        mock.assert(); // Make sure the webhook has been called.

        assert_eq!(
            auth_response
                .unwrap_err()
                .to_string()
                .split('.')
                .collect::<Vec<&str>>()[1]
                .trim(),
            "Only 200 and 401 response status are recognized"
        );
    }

    #[test]
    fn test_all_or_list_serialization_all() {
        let all = AllOrList::<String>::All(All(()));
        let all_json = serde_json::to_string(&all);
        assert_eq!(all_json.as_ref().unwrap(), r#""*""#);
    }

    #[test]
    fn test_all_or_list_serialization_list() {
        let list = AllOrList::<String>::List(vec!["item1".to_string(), "item2".to_string()]);
        let list_json = serde_json::to_string(&list);
        assert_eq!(list_json.as_ref().unwrap(), r#"["item1","item2"]"#);
    }

    #[test]
    fn test_all_or_list_deserialization_all() {
        let all: AllOrList<String> = serde_json::from_str(r#""*""#).unwrap();
        assert!(matches!(all, AllOrList::All(_)));
    }

    #[test]
    fn test_all_or_list_deserialization_list() {
        let list: AllOrList<String> = serde_json::from_str(r#"["item1","item2"]"#).unwrap();
        assert_eq!(
            list,
            AllOrList::List(vec!["item1".to_string(), "item2".to_string()])
        );
    }

    #[test]
    fn test_all_or_list_failed_deserialization_string_value() {
        let result: Result<AllOrList<String>, _> = serde_json::from_str(r#""invalid""#);
        assert!(
            result.is_err(),
            "Deserialization should have failed: {result:#?}"
        );
    }

    #[test]
    fn test_all_or_list_failed_deserialization_object_value() {
        let result: Result<AllOrList<String>, _> = serde_json::from_str(r#""{}""#);
        assert!(
            result.is_err(),
            "Deserialization should have failed: {result:#?}"
        );
    }
}
