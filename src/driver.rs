use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use reqwest::{Client, RequestBuilder};
use serde_json::{json, Map, Value};
use tokio::runtime::Runtime;

use crate::abi::{self, IrodoriConnectorBuffer};
use crate::{ABI_VERSION, CONFIG_JSON, DRIVER_LINKED, ENGINE, MANIFEST_JSON};

static CONNECTIONS: OnceLock<Mutex<HashMap<String, SearchConnection>>> = OnceLock::new();
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

#[derive(Clone)]
struct SearchConnection {
    client: Client,
    config: SearchConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SearchConfig {
    base_url: String,
    username: Option<String>,
    password: Option<String>,
    api_key: Option<String>,
    bearer_token: Option<String>,
    redaction_values: Vec<String>,
}

type QueryRows = Vec<Vec<Value>>;
type QueryOutput = (Vec<String>, QueryRows, bool);

fn connections() -> &'static Mutex<HashMap<String, SearchConnection>> {
    CONNECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime() -> Result<&'static Runtime, String> {
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let runtime = Runtime::new().map_err(|err| format!("create tokio runtime failed: {err}"))?;
    let _ = RUNTIME.set(runtime);
    RUNTIME
        .get()
        .ok_or_else(|| "create tokio runtime failed.".to_string())
}

pub fn call_json(request: IrodoriConnectorBuffer) -> IrodoriConnectorBuffer {
    let request = match abi::parse_request(request) {
        Ok(request) => request,
        Err(response) => return response,
    };
    let method = match abi::request_method(request.as_ref()) {
        Ok(method) => method,
        Err(response) => return response,
    };

    match method {
        "health" | "ping" => abi::ok(Map::from_iter([
            ("engine".to_string(), Value::String(ENGINE.to_string())),
            ("abiVersion".to_string(), json!(ABI_VERSION)),
            ("driverLinked".to_string(), Value::Bool(DRIVER_LINKED)),
        ])),
        "describe" | "capabilities" => abi::ok(Map::from_iter([
            ("engine".to_string(), Value::String(ENGINE.to_string())),
            ("abiVersion".to_string(), json!(ABI_VERSION)),
            ("driverLinked".to_string(), Value::Bool(DRIVER_LINKED)),
            (
                "manifest".to_string(),
                serde_json::from_str(MANIFEST_JSON).unwrap_or(Value::Null),
            ),
            (
                "config".to_string(),
                serde_json::from_str(CONFIG_JSON).unwrap_or(Value::Null),
            ),
        ])),
        "manifest" => abi::owned_buffer(MANIFEST_JSON.to_string()),
        "config" => abi::owned_buffer(CONFIG_JSON.to_string()),
        "connect" => connect(request.as_ref().expect("connect has request")),
        "query" => query(request.as_ref().expect("query has request")),
        "metadata" => metadata(request.as_ref().expect("metadata has request")),
        "close" => close(request.as_ref().expect("close has request")),
        other => abi::error(
            "connector.unknownMethod",
            format!("unknown connector method: {other}"),
        ),
    }
}

fn connect(request: &Value) -> IrodoriConnectorBuffer {
    let connection_id = abi::connection_id(Some(request));
    let config = match SearchConfig::from_request(request) {
        Ok(config) => config,
        Err(err) => return abi::error("connector.invalidRequest", err),
    };
    let connection = SearchConnection {
        client: Client::new(),
        config,
    };
    let version = match runtime().and_then(|runtime| runtime.block_on(load_version(&connection))) {
        Ok(version) => version,
        Err(err) => return abi::error("connector.connectFailed", connection.config.redact(&err)),
    };
    let mut guard = match connections().lock() {
        Ok(guard) => guard,
        Err(_) => {
            return abi::error(
                "connector.statePoisoned",
                "Connector connection state is poisoned.",
            )
        }
    };
    let mut response = Map::from_iter([
        ("engine".to_string(), Value::String(ENGINE.to_string())),
        (
            "connectionId".to_string(),
            Value::String(connection_id.clone()),
        ),
        ("driverLinked".to_string(), Value::Bool(DRIVER_LINKED)),
        (
            "endpoint".to_string(),
            Value::String(connection.config.base_url.clone()),
        ),
    ]);
    if let Some(version) = version {
        response.insert("serverVersion".to_string(), Value::String(version));
    }
    guard.insert(connection_id, connection);
    abi::ok(response)
}

fn query(request: &Value) -> IrodoriConnectorBuffer {
    let connection_id = abi::connection_id(Some(request));
    let Some(statement) = abi::string_field(request, "sql")
        .or_else(|| abi::string_field(request, "query"))
        .or_else(|| abi::string_field(request, "statement"))
    else {
        return abi::error(
            "connector.invalidRequest",
            "query requires a string sql, query, or statement field.",
        );
    };
    let connection = match connection(&connection_id) {
        Ok(connection) => connection,
        Err(response) => return response,
    };
    match runtime().and_then(|runtime| {
        runtime.block_on(run_sql(&connection, statement, abi::max_rows(request)))
    }) {
        Ok((columns, rows, truncated)) => abi::ok(Map::from_iter([
            ("connectionId".to_string(), Value::String(connection_id)),
            (
                "columns".to_string(),
                Value::Array(columns.into_iter().map(Value::String).collect()),
            ),
            (
                "rows".to_string(),
                Value::Array(rows.into_iter().map(Value::Array).collect()),
            ),
            ("truncated".to_string(), Value::Bool(truncated)),
        ])),
        Err(err) => abi::error("connector.queryFailed", connection.config.redact(&err)),
    }
}

fn metadata(request: &Value) -> IrodoriConnectorBuffer {
    let connection_id = abi::connection_id(Some(request));
    let connection = match connection(&connection_id) {
        Ok(connection) => connection,
        Err(response) => return response,
    };
    match runtime().and_then(|runtime| runtime.block_on(load_metadata(&connection))) {
        Ok(metadata) => abi::ok(Map::from_iter([
            ("connectionId".to_string(), Value::String(connection_id)),
            ("metadata".to_string(), metadata),
        ])),
        Err(err) => abi::error("connector.metadataFailed", connection.config.redact(&err)),
    }
}

fn close(request: &Value) -> IrodoriConnectorBuffer {
    let connection_id = abi::connection_id(Some(request));
    let mut guard = match connections().lock() {
        Ok(guard) => guard,
        Err(_) => {
            return abi::error(
                "connector.statePoisoned",
                "Connector connection state is poisoned.",
            )
        }
    };
    let existed = guard.remove(&connection_id).is_some();
    abi::ok(Map::from_iter([
        ("connectionId".to_string(), Value::String(connection_id)),
        ("closed".to_string(), Value::Bool(existed)),
    ]))
}

impl SearchConnection {
    fn auth(&self, builder: RequestBuilder) -> RequestBuilder {
        if let Some(api_key) = self.config.api_key.as_deref() {
            builder.header("Authorization", format!("ApiKey {api_key}"))
        } else if let Some(token) = self.config.bearer_token.as_deref() {
            builder.bearer_auth(token)
        } else if let Some(username) = self.config.username.as_deref() {
            builder.basic_auth(username, self.config.password.as_deref())
        } else {
            builder
        }
    }
}

impl SearchConfig {
    fn from_request(request: &Value) -> Result<Self, String> {
        let base_url = option_string(request, &["connectionString", "url", "dsn"])
            .unwrap_or_else(|| build_url(request));
        let username = option_string(request, &["user", "username"]);
        let password = option_string(request, &["password"]);
        let api_key = option_string(request, &["apiKey", "api_key"]);
        let bearer_token = option_string(request, &["token", "bearerToken", "accessToken"]);
        let mut redaction_values = Vec::new();
        push_sensitive(&mut redaction_values, password.as_deref());
        push_sensitive(&mut redaction_values, api_key.as_deref());
        push_sensitive(&mut redaction_values, bearer_token.as_deref());
        collect_url_auth(&base_url, &mut redaction_values);
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            username,
            password,
            api_key,
            bearer_token,
            redaction_values,
        })
    }

    fn redact(&self, message: &str) -> String {
        self.redaction_values.iter().fold(
            message.replace(&self.base_url, "<search-url>"),
            |message, secret| {
                if secret.is_empty() {
                    message
                } else {
                    message.replace(secret, "****")
                }
            },
        )
    }
}

async fn load_version(connection: &SearchConnection) -> Result<Option<String>, String> {
    let response = connection
        .auth(connection.client.get(&connection.config.base_url))
        .send()
        .await
        .map_err(|err| format!("{ENGINE} root request failed: {err}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|err| format!("{ENGINE} response read failed: {err}"))?;
    if !status.is_success() {
        return Err(format!(
            "{ENGINE} root request returned HTTP {status}: {text}"
        ));
    }
    let value = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
    Ok(value
        .get("version")
        .and_then(|version| version.get("number"))
        .and_then(Value::as_str)
        .map(|version| format!("{ENGINE} {version}")))
}

async fn run_sql(
    connection: &SearchConnection,
    statement: &str,
    cap: usize,
) -> Result<QueryOutput, String> {
    let payload = json!({ "query": statement });
    let endpoints = if ENGINE == "opensearch" {
        vec![
            "/_plugins/_sql?format=json",
            "/_opendistro/_sql?format=json",
        ]
    } else {
        vec!["/_sql?format=json"]
    };
    let mut last_error = None;
    for endpoint in endpoints {
        let response = connection
            .auth(
                connection
                    .client
                    .post(format!("{}{}", connection.config.base_url, endpoint)),
            )
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|err| format!("{ENGINE} SQL request failed: {err}"))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|err| format!("{ENGINE} SQL response read failed: {err}"))?;
        if status.is_success() {
            return Ok(sql_response_to_output(&text, cap));
        }
        last_error = Some(format!(
            "{ENGINE} SQL returned HTTP {status}: {}",
            text.trim().chars().take(500).collect::<String>()
        ));
        if status.as_u16() != 404 {
            break;
        }
    }
    Err(last_error.unwrap_or_else(|| format!("{ENGINE} SQL request failed.")))
}

async fn load_metadata(connection: &SearchConnection) -> Result<Value, String> {
    let response = connection
        .auth(connection.client.get(format!(
            "{}/_cat/indices?format=json&h=index,health,status,docs.count,store.size",
            connection.config.base_url
        )))
        .send()
        .await
        .map_err(|err| format!("{ENGINE} index metadata request failed: {err}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|err| format!("{ENGINE} metadata response read failed: {err}"))?;
    if !status.is_success() {
        return Err(format!("{ENGINE} metadata returned HTTP {status}: {text}"));
    }
    let indices = serde_json::from_str::<Vec<Value>>(&text).unwrap_or_default();
    let mut objects = Vec::new();
    for index in indices {
        let Some(name) = index.get("index").and_then(Value::as_str) else {
            continue;
        };
        let columns = load_index_columns(connection, name).await.unwrap_or_else(|_| {
            vec![
                json!({"name": "_id", "dataType": "keyword", "nullable": false, "ordinal": 1}),
                json!({"name": "_source", "dataType": "object", "nullable": true, "ordinal": 2}),
            ]
        });
        objects.push(json!({
            "schema": "default",
            "name": name,
            "kind": "index",
            "columns": columns,
            "indexes": [],
            "primaryKey": [{"name": "_id", "keyType": "primary"}],
            "foreignKeys": [],
            "health": index.get("health").cloned().unwrap_or(Value::Null),
            "status": index.get("status").cloned().unwrap_or(Value::Null),
            "documentCount": index.get("docs.count").cloned().unwrap_or(Value::Null),
            "storeSize": index.get("store.size").cloned().unwrap_or(Value::Null)
        }));
    }
    Ok(json!({ "schemas": [{ "name": "default", "objects": objects }] }))
}

async fn load_index_columns(
    connection: &SearchConnection,
    index: &str,
) -> Result<Vec<Value>, String> {
    let response = connection
        .auth(
            connection
                .client
                .get(format!("{}/{}/_mapping", connection.config.base_url, index)),
        )
        .send()
        .await
        .map_err(|err| format!("{ENGINE} mapping request failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "{ENGINE} mapping returned HTTP {}",
            response.status()
        ));
    }
    let value = response
        .json::<Value>()
        .await
        .map_err(|err| format!("{ENGINE} mapping JSON failed: {err}"))?;
    let properties = value
        .get(index)
        .and_then(|index| index.get("mappings"))
        .and_then(|mappings| mappings.get("properties"))
        .and_then(Value::as_object);
    let mut columns = vec![json!({
        "name": "_id",
        "dataType": "keyword",
        "nullable": false,
        "ordinal": 1
    })];
    if let Some(properties) = properties {
        flatten_properties("", properties, &mut columns);
    }
    Ok(columns)
}

fn flatten_properties(prefix: &str, properties: &Map<String, Value>, columns: &mut Vec<Value>) {
    for (name, definition) in properties {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}.{name}")
        };
        let data_type = definition
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("object");
        columns.push(json!({
            "name": path,
            "dataType": data_type,
            "nullable": true,
            "ordinal": columns.len() + 1
        }));
        if let Some(child) = definition.get("properties").and_then(Value::as_object) {
            flatten_properties(&path, child, columns);
        }
    }
}

fn sql_response_to_output(text: &str, cap: usize) -> QueryOutput {
    let value = serde_json::from_str::<Value>(text).unwrap_or(Value::Null);
    if let Some(columns) = value.get("columns").and_then(Value::as_array) {
        let names = columns
            .iter()
            .map(|column| {
                column
                    .get("name")
                    .or_else(|| column.get("column"))
                    .and_then(Value::as_str)
                    .unwrap_or("value")
                    .to_string()
            })
            .collect::<Vec<_>>();
        let rows = value
            .get("rows")
            .or_else(|| value.get("datarows"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        return rows_to_output(names, rows, cap);
    }
    if let Some(schema) = value.get("schema").and_then(Value::as_array) {
        let names = schema
            .iter()
            .map(|column| {
                column
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("value")
                    .to_string()
            })
            .collect::<Vec<_>>();
        let rows = value
            .get("datarows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        return rows_to_output(names, rows, cap);
    }
    rows_from_json(value, cap)
}

fn rows_to_output(columns: Vec<String>, rows: Vec<Value>, cap: usize) -> QueryOutput {
    let mut output = Vec::new();
    let mut truncated = false;
    for row in rows {
        if output.len() >= cap {
            truncated = true;
            break;
        }
        output.push(match row {
            Value::Array(values) => values,
            other => vec![other],
        });
    }
    (columns, output, truncated)
}

fn rows_from_json(value: Value, cap: usize) -> QueryOutput {
    let rows = value
        .get("hits")
        .and_then(|hits| hits.get("hits"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| vec![value]);
    let mut columns = Vec::new();
    for row in &rows {
        if let Some(object) = row.as_object() {
            for key in object.keys() {
                if !columns.iter().any(|column| column == key) {
                    columns.push(key.clone());
                }
            }
        }
    }
    if columns.is_empty() {
        columns.push("value".to_string());
    }
    let output = rows
        .iter()
        .take(cap)
        .map(|row| {
            if let Some(object) = row.as_object() {
                columns
                    .iter()
                    .map(|column| object.get(column).cloned().unwrap_or(Value::Null))
                    .collect()
            } else {
                vec![row.clone()]
            }
        })
        .collect::<Vec<_>>();
    (columns, output, rows.len() > cap)
}

fn build_url(request: &Value) -> String {
    let host = option_string(request, &["host", "endpoint"]).unwrap_or_else(|| "127.0.0.1".into());
    let port = option_string(request, &["port"]).unwrap_or_else(|| {
        if ENGINE == "opensearch" {
            "9200".into()
        } else {
            "9200".into()
        }
    });
    let scheme = if bool_option(request, &["tls", "ssl"]).unwrap_or(false) {
        "https"
    } else {
        "http"
    };
    format!("{scheme}://{host}:{port}")
}

fn connection(connection_id: &str) -> Result<SearchConnection, IrodoriConnectorBuffer> {
    let guard = connections().lock().map_err(|_| {
        abi::error(
            "connector.statePoisoned",
            "Connector connection state is poisoned.",
        )
    })?;
    guard.get(connection_id).cloned().ok_or_else(|| {
        abi::error(
            "connector.connectionNotFound",
            format!("no open connection: {connection_id}"),
        )
    })
}

fn request_containers(request: &Value) -> Vec<&Value> {
    [
        Some(request),
        request.get("profile"),
        request.get("options"),
        request.get("auth"),
        request.get("secrets"),
        request
            .get("profile")
            .and_then(|profile| profile.get("options")),
        request
            .get("profile")
            .and_then(|profile| profile.get("auth")),
        request
            .get("profile")
            .and_then(|profile| profile.get("secrets")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn option_string(request: &Value, fields: &[&str]) -> Option<String> {
    request_containers(request)
        .into_iter()
        .find_map(|container| {
            fields.iter().find_map(|field| {
                container
                    .get(*field)
                    .map(|value| match value {
                        Value::String(value) => value.clone(),
                        Value::Number(value) => value.to_string(),
                        Value::Bool(value) => value.to_string(),
                        _ => String::new(),
                    })
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
        })
}

fn bool_option(request: &Value, fields: &[&str]) -> Option<bool> {
    request_containers(request)
        .into_iter()
        .find_map(|container| {
            fields
                .iter()
                .find_map(|field| container.get(*field).and_then(Value::as_bool))
        })
}

fn push_sensitive(values: &mut Vec<String>, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        if !values.iter().any(|existing| existing == value) {
            values.push(value.to_string());
        }
    }
}

fn collect_url_auth(url: &str, values: &mut Vec<String>) {
    let Some(after_scheme) = url.split_once("://").map(|(_, rest)| rest) else {
        return;
    };
    let Some(auth) = after_scheme
        .split('/')
        .next()
        .and_then(|host| host.split('@').next())
    else {
        return;
    };
    if auth.contains(':') {
        for part in auth.split(':') {
            push_sensitive(values, Some(part));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_elasticsearch_sql_response() {
        let (columns, rows, truncated) = sql_response_to_output(
            r#"{"columns":[{"name":"name"},{"name":"count"}],"rows":[["a",2]]}"#,
            10,
        );
        assert_eq!(columns, vec!["name", "count"]);
        assert_eq!(rows, vec![vec![json!("a"), json!(2)]]);
        assert!(!truncated);
    }

    #[test]
    fn parses_opensearch_sql_response() {
        let (columns, rows, truncated) = sql_response_to_output(
            r#"{"schema":[{"name":"name"}],"datarows":[["a"],["b"]]}"#,
            1,
        );
        assert_eq!(columns, vec!["name"]);
        assert_eq!(rows, vec![vec![json!("a")]]);
        assert!(truncated);
    }

    #[test]
    fn builds_url_from_profile() {
        let request = json!({"profile": {"host": "search.local", "port": 9443, "tls": true}});
        let config = SearchConfig::from_request(&request).unwrap();
        assert_eq!(config.base_url, "https://search.local:9443");
    }
}
