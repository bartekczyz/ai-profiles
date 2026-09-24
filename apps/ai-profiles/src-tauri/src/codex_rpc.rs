//! A client for `codex app-server`, the JSON-RPC server the Codex CLI runs
//! over stdio: one JSON message per line each way, with the server's
//! notifications interleaved between the responses.
//!
//! App-server works on the `CODEX_HOME` it is started with, using that home's
//! own auth and token refresh. Its methods are undocumented internals, so
//! callers read what they get back leniently.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Map, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// How long app-server gets to answer one request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The name ai-profiles introduces itself to app-server with.
const CLIENT_NAME: &str = "ai-profiles";

/// Why a request to app-server failed.
#[derive(Debug, thiserror::Error)]
pub enum CodexRpcError {
    /// The `codex` binary isn't on the user's shell PATH.
    #[error("the codex binary isn't on PATH")]
    NotInstalled,
    /// Starting app-server, or writing to or reading from it, failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// App-server closed its output before answering.
    #[error("codex app-server exited before answering")]
    Closed,
    /// App-server didn't answer within [`REQUEST_TIMEOUT`].
    #[error("codex app-server didn't answer in time")]
    Timeout,
    /// App-server answered with a JSON-RPC error.
    #[error("{0}")]
    Rpc(String),
    /// App-server answered with something the caller can't read.
    #[error("unexpected answer from codex app-server: {0}")]
    Unexpected(String),
}

/// Something that sends app-server requests and waits for their results: a
/// running [`CodexRpc`], or a stand-in in tests.
#[async_trait]
pub trait CodexTransport: Send {
    /// Call `method` with `params` (`null` for none) and return the result.
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, CodexRpcError>;
}

/// A running `codex app-server`, initialized and ready for requests. Dropping
/// it kills the server.
pub struct CodexRpc {
    /// The server process, killed when this is dropped.
    _child: Child,
    /// The server's input. It stays open for the client's whole life: the
    /// server stops writing the moment its input closes (verified live against
    /// v0.135.0: closing it right after a request yields no output at all), so
    /// closing it before a response arrived would lose the response.
    stdin: ChildStdin,
    /// The server's output, line by line.
    lines: Lines<BufReader<ChildStdout>>,
    /// The id of the last request sent.
    last_id: i64,
    /// How long the server gets to answer one request.
    timeout: Duration,
}

impl CodexRpc {
    /// Start app-server on `codex_home` and initialize it: an error answer to
    /// `initialize` fails the start.
    pub async fn start(codex_home: &Path) -> Result<Self, CodexRpcError> {
        Self::connect(server_command(codex_home)?, REQUEST_TIMEOUT).await
    }

    /// Start app-server on `codex_home` and introduce ai-profiles to it
    /// without waiting for its answer, which the first request's then comes
    /// after: that saves a round trip, and leaves it to that request to say
    /// whether the server works. See [`CodexRpc::introduce`].
    pub async fn launch(codex_home: &Path) -> Result<Self, CodexRpcError> {
        Self::introduce(server_command(codex_home)?, REQUEST_TIMEOUT).await
    }

    /// Spawn `command` as the server, with piped stdio, and initialize it,
    /// giving it `timeout` to answer each request.
    async fn connect(command: Command, timeout: Duration) -> Result<Self, CodexRpcError> {
        let mut rpc = Self::spawn(command, timeout)?;
        rpc.request("initialize", client_info()).await?;
        rpc.send(&json!({ "jsonrpc": "2.0", "method": "initialized" }))
            .await?;
        Ok(rpc)
    }

    /// Spawn `command` as the server, with piped stdio, and introduce
    /// ai-profiles to it, giving it `timeout` to answer each request.
    ///
    /// `initialize` gets id 1 and its answer is never waited for: each
    /// request reads past it, as it reads past anything not answering its own
    /// id, so an error answer to it fails nothing by itself.
    pub(crate) async fn introduce(
        command: Command,
        timeout: Duration,
    ) -> Result<Self, CodexRpcError> {
        let mut rpc = Self::spawn(command, timeout)?;
        rpc.last_id += 1;
        let initialize = json!({
            "jsonrpc": "2.0",
            "id": rpc.last_id,
            "method": "initialize",
            "params": client_info(),
        });
        rpc.send(&initialize).await?;
        rpc.send(&json!({ "jsonrpc": "2.0", "method": "initialized" }))
            .await?;
        Ok(rpc)
    }

    /// Spawn `command` as the server, with piped stdio, giving it `timeout`
    /// to answer each request.
    fn spawn(mut command: Command, timeout: Duration) -> Result<Self, CodexRpcError> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let stdin = child.stdin.take().ok_or(CodexRpcError::Closed)?;
        let stdout = child.stdout.take().ok_or(CodexRpcError::Closed)?;
        Ok(Self {
            _child: child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            last_id: 0,
            timeout,
        })
    }

    /// Write `message` to the server as one line.
    async fn send(&mut self, message: &Value) -> Result<(), CodexRpcError> {
        let mut line = message.to_string();
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }
}

#[async_trait]
impl CodexTransport for CodexRpc {
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, CodexRpcError> {
        self.last_id += 1;
        let id = self.last_id;
        let mut message = Map::new();
        message.insert("jsonrpc".to_string(), json!("2.0"));
        message.insert("id".to_string(), json!(id));
        message.insert("method".to_string(), json!(method));
        if !params.is_null() {
            message.insert("params".to_string(), params);
        }
        self.send(&Value::Object(message)).await?;
        let lines = &mut self.lines;
        let response = async {
            while let Some(line) = lines.next_line().await? {
                if let Some(outcome) = extract_result_for_id(&line, id) {
                    return outcome;
                }
            }
            Err(CodexRpcError::Closed)
        };
        tokio::time::timeout(self.timeout, response)
            .await
            .map_err(|_| CodexRpcError::Timeout)?
    }
}

/// The command running app-server on `codex_home`.
///
/// A Tauri app launched from Finder doesn't inherit the shell PATH, so a bare
/// `codex` spawn would fail with ENOENT. The binary is resolved to an absolute
/// path, and the resolved PATH passed on, so this works however ai-profiles
/// was launched.
fn server_command(codex_home: &Path) -> Result<Command, CodexRpcError> {
    let binary =
        crate::deps::resolve_cli_binary_path("codex").ok_or(CodexRpcError::NotInstalled)?;
    let mut command = Command::new(binary);
    command
        .arg("app-server")
        .env("CODEX_HOME", codex_home)
        .env("PATH", crate::deps::shell_path());
    Ok(command)
}

/// The `initialize` params introducing ai-profiles.
fn client_info() -> Value {
    json!({
        "clientInfo": { "name": CLIENT_NAME, "version": env!("CARGO_PKG_VERSION") },
    })
}

/// The outcome of the request with `id` if `line` is its response: its
/// `result`, or its `error` as [`CodexRpcError::Rpc`]. `None` for anything
/// else the server writes: notifications, requests of its own (which carry a
/// `method`, and ids from their own sequence), and lines that aren't JSON.
fn extract_result_for_id(line: &str, id: i64) -> Option<Result<Value, CodexRpcError>> {
    let mut message: Value = serde_json::from_str(line).ok()?;
    if message.get("id").and_then(Value::as_i64) != Some(id) || message.get("method").is_some() {
        return None;
    }
    if let Some(error) = message.get("error") {
        let text = error
            .get("message")
            .and_then(Value::as_str)
            .map_or_else(|| error.to_string(), str::to_string);
        return Some(Err(CodexRpcError::Rpc(text)));
    }
    message.get_mut("result").map(|result| Ok(result.take()))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn extract_matching_response_skips_interleaved_notifications() {
        let lines = [
            r#"{"id":1,"result":{"codexHome":"/x"}}"#,
            r#"{"method":"remoteControl/status/changed","params":{}}"#,
            r#"{"id":2,"method":"item/commandExecution/requestApproval","params":{}}"#,
            "not json",
            r#"{"id":2,"result":{"rateLimits":{"primary":{"usedPercent":5}}}}"#,
        ];

        let outcomes: Vec<Option<Value>> = lines
            .iter()
            .map(|line| extract_result_for_id(line, 2).map(Result::unwrap))
            .collect();

        assert_eq!(
            outcomes,
            [
                None,
                None,
                None,
                None,
                Some(json!({ "rateLimits": { "primary": { "usedPercent": 5 } } })),
            ]
        );
    }

    #[test]
    fn extract_returns_a_jsonrpc_error_with_its_message() {
        let line = r#"{"id":2,"error":{"code":-32000,"message":"not signed in"}}"#;

        let outcome = extract_result_for_id(line, 2);

        assert!(
            matches!(outcome, Some(Err(CodexRpcError::Rpc(message))) if message == "not signed in")
        );
    }

    /// A stand-in for app-server: answers `initialize`, then each request with
    /// its method name, preceded by a notification and a server request that
    /// reuses the id; `fail` gets an error and `quit` makes it exit.
    const FAKE_SERVER: &str = r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  method=$(printf '%s' "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
  case "$method" in
    initialized) ;;
    quit) exit 0 ;;
    fail) echo "{\"id\":$id,\"error\":{\"code\":-1,\"message\":\"nope\"}}" ;;
    *)
      echo '{"method":"note","params":{}}'
      echo "{\"id\":$id,\"method\":\"approve\",\"params\":{}}"
      echo "{\"id\":$id,\"result\":{\"method\":\"$method\",\"line\":$line}}"
      ;;
  esac
done
"#;

    async fn fake_server(dir: &Path) -> CodexRpc {
        let script = dir.join("app-server");
        std::fs::write(&script, FAKE_SERVER).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        CodexRpc::connect(Command::new(script), Duration::from_secs(5))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn requests_get_the_result_answering_their_id() {
        let dir = tempdir().unwrap();
        let mut rpc = fake_server(dir.path()).await;

        let first = rpc
            .request("thread/list", json!({ "archived": true }))
            .await
            .unwrap();
        let second = rpc
            .request("account/rateLimits/read", Value::Null)
            .await
            .unwrap();

        assert_eq!(first["method"], "thread/list");
        assert_eq!(first["line"]["params"], json!({ "archived": true }));
        assert_eq!(first["line"]["id"], 2);
        assert_eq!(second["method"], "account/rateLimits/read");
        assert_eq!(second["line"]["id"], 3);
        assert!(second["line"].get("params").is_none());
    }

    #[tokio::test]
    async fn an_error_response_and_an_exit_fail_the_request() {
        let dir = tempdir().unwrap();
        let mut rpc = fake_server(dir.path()).await;

        let failed = rpc.request("fail", Value::Null).await;
        let quit = rpc.request("quit", Value::Null).await;

        assert!(matches!(failed, Err(CodexRpcError::Rpc(message)) if message == "nope"));
        assert!(matches!(quit, Err(CodexRpcError::Closed)));
    }
}
