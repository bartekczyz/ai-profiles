//! Stand-ins the Codex session tests share: homes, rollouts, writer locks, a
//! running desktop app and a scripted app-server.

use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::app_kind::AppKind;
use crate::codex_rpc::{CodexRpcError, CodexTransport};
use crate::sessions::Home;

/// A managed Codex home, `Personal`, under `root`, the stock one if `stock`.
pub fn home(root: &Path, stock: bool) -> Home {
    Home {
        id: "personal".to_string(),
        app: AppKind::Codex,
        label: "Personal".to_string(),
        config_dir: root.join("cli-config"),
        gui_data_dir: root.join("gui-data"),
        stock,
    }
}

/// Writes rollout `id`, started by `originator`, into the sessions of the home
/// under `root`. Returns its path.
pub fn write_rollout(root: &Path, id: &str, originator: &str) -> PathBuf {
    let dir = root
        .join("cli-config")
        .join("sessions")
        .join("2026")
        .join("09")
        .join("01");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("rollout-2026-09-01T10-00-00-{id}.jsonl"));
    let meta = json!({
        "type": "session_meta",
        "payload": { "id": id, "originator": originator, "base_instructions": { "text": "…" } },
    });
    fs::write(&path, format!("{meta}\n{{\"type\":\"event_msg\"}}\n")).unwrap();
    path
}

/// Leaves a writer lock of thread `id` in `home`, held by nothing.
pub fn write_lock(home: &Home, id: &str) {
    let dir = home.config_dir.join("thread-writer-locks");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{id}.lock")), "").unwrap();
}

/// `ps` output with `home`'s desktop app running.
pub fn running_desktop(home: &Home) -> String {
    format!(
        "  900 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT --user-data-dir={}\n",
        home.gui_data_dir.display()
    )
}

/// A stand-in transport that answers each call with `respond`, recording
/// every call it receives.
pub struct ScriptedServer<F> {
    /// Answers a call to a method with its params.
    respond: F,
    /// Every call received, in order: its method and params.
    pub calls: Vec<(String, Value)>,
}

impl<F> ScriptedServer<F>
where
    F: FnMut(&str, &Value) -> Result<Value, CodexRpcError>,
{
    /// A server answering with `respond`.
    pub fn new(respond: F) -> Self {
        Self {
            respond,
            calls: Vec::new(),
        }
    }
}

#[async_trait]
impl<F> CodexTransport for ScriptedServer<F>
where
    F: FnMut(&str, &Value) -> Result<Value, CodexRpcError> + Send,
{
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, CodexRpcError> {
        self.calls.push((method.to_string(), params.clone()));
        (self.respond)(method, &params)
    }
}

/// A `thread/read` response for a thread `id`, at `path`, with `status`.
pub fn read_response(id: &str, path: Option<&Path>, status: Option<&str>) -> Value {
    json!({
        "thread": {
            "id": id,
            "path": path,
            "status": status.map(|kind| json!({ "type": kind })),
        }
    })
}
