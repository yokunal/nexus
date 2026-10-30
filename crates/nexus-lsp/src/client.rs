use super::protocol::{
    HoverInfo, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, Location, SymbolInfo,
};
use super::transport::{LspTransport, TransportError};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::io::BufReader;
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::{timeout, Duration};

#[derive(Error, Debug)]
pub enum LspError {
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("LSP server error ({code}): {message}")]
    Server { code: i64, message: String },
    #[error("Request timed out")]
    Timeout,
    #[error("Server not running")]
    NotRunning,
}

pub struct LspClient {
    server_cmd: String,
    workspace_root: PathBuf,
    stdin_tx: mpsc::Sender<String>,
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    next_id: AtomicU64,
    _child: Arc<Mutex<Child>>,
}

impl LspClient {
    /// Launches a language server subprocess and starts background message pump.
    pub async fn start(
        server_cmd: &str,
        workspace_root: impl Into<PathBuf>,
    ) -> Result<Self, LspError> {
        let root = workspace_root.into();
        let canonical_root = root.canonicalize().unwrap_or(root);

        let mut child = Command::new(server_cmd)
            .current_dir(&canonical_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("Failed to capture server stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("Failed to capture server stdout"))?;

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(64);
        let pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Writer background task
        tokio::spawn(async move {
            let mut writer = stdin;
            while let Some(msg) = stdin_rx.recv().await {
                if LspTransport::write_message(&mut writer, &msg)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        // Reader background task
        let pending_clone = pending_requests.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            while let Ok(text) = LspTransport::read_message(&mut reader).await {
                if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(&text) {
                    if let Some(id) = resp.id {
                        let mut map = pending_clone.lock().await;
                        if let Some(sender) = map.remove(&id) {
                            let _ = sender.send(resp);
                        }
                    }
                }
            }
        });

        let client = Self {
            server_cmd: server_cmd.to_string(),
            workspace_root: canonical_root,
            stdin_tx,
            pending_requests,
            next_id: AtomicU64::new(1),
            _child: Arc::new(Mutex::new(child)),
        };

        client.initialize().await?;
        Ok(client)
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn server_cmd(&self) -> &str {
        &self.server_cmd
    }

    async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse, LspError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let req_json = serde_json::to_string(&req)?;
        let (resp_tx, resp_rx) = oneshot::channel();

        {
            let mut map = self.pending_requests.lock().await;
            map.insert(id, resp_tx);
        }

        self.stdin_tx
            .send(req_json)
            .await
            .map_err(|_| LspError::NotRunning)?;

        match timeout(Duration::from_secs(10), resp_rx).await {
            Ok(Ok(resp)) => {
                if let Some(err) = resp.error {
                    Err(LspError::Server {
                        code: err.code,
                        message: err.message,
                    })
                } else {
                    Ok(resp)
                }
            }
            Ok(Err(_)) => Err(LspError::NotRunning),
            Err(_) => {
                let mut map = self.pending_requests.lock().await;
                map.remove(&id);
                Err(LspError::Timeout)
            }
        }
    }

    async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), LspError> {
        let notif = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        };
        let notif_json = serde_json::to_string(&notif)?;
        self.stdin_tx
            .send(notif_json)
            .await
            .map_err(|_| LspError::NotRunning)?;
        Ok(())
    }

    /// Performs LSP handshake: initialize + initialized notification
    async fn initialize(&self) -> Result<(), LspError> {
        let root_uri = format!("file://{}", self.workspace_root.display());
        let params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "rootPath": self.workspace_root.display().to_string(),
            "capabilities": {
                "textDocument": {
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "definition": { "dynamicRegistration": false },
                    "documentSymbol": { "hierarchicalDocumentSymbolSupport": true }
                }
            }
        });

        self.send_request("initialize", Some(params)).await?;
        self.send_notification("initialized", Some(json!({})))
            .await?;
        Ok(())
    }

    /// Notifies LSP of an open file and its content
    pub async fn did_open_file(&self, path: &Path, content: &str) -> Result<(), LspError> {
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace_root.join(path)
        };
        let uri = format!("file://{}", full_path.display());

        let params = json!({
            "textDocument": {
                "uri": uri,
                "languageId": "rust",
                "version": 1,
                "text": content
            }
        });

        self.send_notification("textDocument/didOpen", Some(params))
            .await?;
        Ok(())
    }

    /// Queries type signatures and documentation at a file position
    pub async fn hover(
        &self,
        path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Option<HoverInfo>, LspError> {
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace_root.join(path)
        };
        let uri = format!("file://{}", full_path.display());

        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": line.saturating_sub(1), "character": character }
        });

        let resp = self
            .send_request("textDocument/hover", Some(params))
            .await?;
        let result = match resp.result {
            Some(r) if !r.is_null() => r,
            _ => return Ok(None),
        };

        let contents_val = &result["contents"];
        let contents = if let Some(s) = contents_val.as_str() {
            s.to_string()
        } else if let Some(val) = contents_val.get("value") {
            val.as_str().unwrap_or_default().to_string()
        } else {
            serde_json::to_string_pretty(contents_val).unwrap_or_default()
        };

        Ok(Some(HoverInfo {
            contents,
            range: None,
        }))
    }

    /// Finds symbol definition location
    pub async fn definition(
        &self,
        path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<Location>, LspError> {
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace_root.join(path)
        };
        let uri = format!("file://{}", full_path.display());

        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": line.saturating_sub(1), "character": character }
        });

        let resp = self
            .send_request("textDocument/definition", Some(params))
            .await?;
        let result = match resp.result {
            Some(r) if !r.is_null() => r,
            _ => return Ok(Vec::new()),
        };

        let mut locations = Vec::new();
        if let Some(arr) = result.as_array() {
            for item in arr {
                if let Ok(loc) = serde_json::from_value::<Location>(item.clone()) {
                    locations.push(loc);
                }
            }
        } else if let Ok(loc) = serde_json::from_value::<Location>(result) {
            locations.push(loc);
        }

        Ok(locations)
    }

    /// Lists symbols (structs, functions, traits, enums) defined in a file
    pub async fn document_symbols(&self, path: &Path) -> Result<Vec<SymbolInfo>, LspError> {
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace_root.join(path)
        };
        let uri = format!("file://{}", full_path.display());

        let params = json!({
            "textDocument": { "uri": uri }
        });

        let resp = self
            .send_request("textDocument/documentSymbol", Some(params))
            .await?;
        let result = match resp.result {
            Some(r) if !r.is_null() => r,
            _ => return Ok(Vec::new()),
        };

        let mut symbols = Vec::new();
        if let Some(arr) = result.as_array() {
            for item in arr {
                let name = item["name"].as_str().unwrap_or_default().to_string();
                let kind = item["kind"].as_u64().unwrap_or(0) as u32;
                let container_name = item["containerName"].as_str().map(|s| s.to_string());
                symbols.push(SymbolInfo {
                    name,
                    kind,
                    location: None,
                    container_name,
                });
            }
        }

        Ok(symbols)
    }
}
