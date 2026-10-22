//! MCP stdio server for peri: LayerFS Store tools.
//!
//! Store calls run in this process through `layerfs-store-service`, the same
//! route the JavaScript SDK's native binding uses. Sandbox Workspaces need the
//! Linux daemon and are not part of this binary.
use layerfs_api::{InitRequest, ProjectApi};
use layerfs_bridge::control::{Answer, Reply, SdkCall, SdkRefusal, SdkValue};
use layerfs_bridge::provision::StoreManifest;
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use layerfs_store_service::{install, StoreService};
use layerfs_telemetry::timer::Timing;
use rmcp::model::{
    CallToolResult, ClientNotification, ClientRequest, ContentBlock, EmptyResult, ErrorCode,
    Implementation, JsonObject, ListToolsResult, ProtocolVersion, ServerCapabilities, ServerConfig,
    ServerResult, Tool,
};
use rmcp::service::{NotificationContext, RequestContext};
use rmcp::transport::io::stdio;
use rmcp::{ErrorData as McpError, RoleServer, Service, ServiceExt};
use serde_json::{json, Value as Json};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SERVER: &str = "layerfs";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const INSTRUCTIONS: &str = "LayerFS Store tools. Call project_init with a source path, or store_open with an existing project record. workspace.exec needs the Linux Sandbox bridge and is not exposed here.";
const LOCATOR: &str = "/layerfs-store/global/store.sqlite";
const INIT_DEADLINE: Duration = Duration::from_secs(60);

struct App {
    service: Option<StoreService>,
    project: Option<String>,
    store: Option<PathBuf>,
}

struct LayerfsServer {
    app: Mutex<App>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The hand-written server handled one request at a time, and Store calls
    // depend on that ordering (fork -> write -> merge). A current-thread
    // runtime keeps rmcp's spawned handlers serialized.
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App {
        service: None,
        project: None,
        store: None,
    };
    if let Err(error) = app.open_from_env() {
        eprintln!("layerfs-mcp: {error}");
    }
    let server = LayerfsServer {
        app: Mutex::new(app),
    };
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

impl Service<RoleServer> for LayerfsServer {
    async fn handle_request(
        &self,
        request: ClientRequest,
        _context: RequestContext<RoleServer>,
    ) -> Result<ServerResult, McpError> {
        let result = match request {
            ClientRequest::InitializeRequest(_) => ServerResult::InitializeResult(self.get_info()),
            ClientRequest::PingRequest(_) => ServerResult::EmptyResult(EmptyResult {}),
            ClientRequest::ListToolsRequest(_) => {
                ServerResult::ListToolsResult(ListToolsResult::with_all_items(tools()))
            }
            ClientRequest::CallToolRequest(request) => {
                let args = request
                    .params
                    .arguments
                    .map(Json::Object)
                    .unwrap_or_else(|| json!({}));
                let mut app = self
                    .app
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                ServerResult::CallToolResult(app.call_tool(&request.params.name, &args))
            }
            // peri's client probes 2026-07-28 discovery first; refusing it
            // makes rmcp fall back to the legacy initialize handshake. This
            // must bypass the request-metadata checks in `ServerHandler`,
            // which would reject the probe as an unsupported version.
            ClientRequest::DiscoverRequest(_) | ClientRequest::CustomRequest(_) => {
                return Err(method_not_found())
            }
            _ => return Err(method_not_found()),
        };
        let mut result = result;
        // The session always negotiates 2025-06-18, where `resultType` did not
        // exist yet.
        result.strip_result_type_for_legacy_peer();
        Ok(result)
    }

    async fn handle_notification(
        &self,
        _notification: ClientNotification,
        _context: NotificationContext<RoleServer>,
    ) -> Result<(), McpError> {
        Ok(())
    }

    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2025_06_18)
            .with_server_info(Implementation::new(SERVER, VERSION))
            .with_instructions(INSTRUCTIONS)
    }

    // The hand-written server only ever spoke 2025-06-18. Keep the same
    // answer for clients that request a newer revision.
    fn supported_protocol_versions(&self) -> std::borrow::Cow<'static, [ProtocolVersion]> {
        std::borrow::Cow::Owned(vec![ProtocolVersion::V_2025_06_18])
    }
}

fn method_not_found() -> McpError {
    McpError::new(ErrorCode::METHOD_NOT_FOUND, "method not found", None)
}

impl App {
    fn open_from_env(&mut self) -> Result<(), String> {
        let project = match std::env::var("LAYERFS_PROJECT") {
            Ok(project) if !project.is_empty() => project,
            _ => return Ok(()),
        };
        let home = std::env::var_os("LAYERFS_HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "LAYERFS_HOME is required with LAYERFS_PROJECT".to_owned())?;
        let store = std::env::var_os("LAYERFS_STORE_PATH")
            .map(PathBuf::from)
            .ok_or_else(|| "LAYERFS_STORE_PATH is required with LAYERFS_PROJECT".to_owned())?;
        self.open_record(&project, &home, &store)
    }

    fn open_record(&mut self, project: &str, home: &Path, store: &Path) -> Result<(), String> {
        let record = std::fs::read(home.join(project).join("project.record"))
            .map_err(|error| format!("project record: {error}"))?;
        let answer = Answer::decode(&record).map_err(|error| error.to_string())?;
        let value = match answer.reply {
            Reply::Sdk(value) => value,
            _ => return Err("project record kind".into()),
        };
        let installed = match value.get("installed") {
            Some(SdkValue::Bytes(bytes)) => {
                StoreManifest::decode(bytes).map_err(|error| error.to_string())?
            }
            _ => return Err("project has no installed manifest".into()),
        };
        let template = overlay_template()?;
        let service =
            StoreService::open_direct(store, &installed.binding, installed.cursor_key, &template)
                .map_err(|error| format!("Store open: {error:?}"))?;
        self.service = Some(service);
        self.project = Some(project.to_owned());
        self.store = Some(store.to_owned());
        Ok(())
    }

    fn call_tool(&mut self, name: &str, args: &Json) -> CallToolResult {
        match name {
            "project_init" => match self.project_init(args) {
                Ok(value) => tool_text(value),
                Err(error) => tool_error(&error),
            },
            "store_open" => match self.store_open(args) {
                Ok(value) => tool_text(value),
                Err(error) => tool_error(&error),
            },
            _ => {
                let service = match self.service.as_ref() {
                    Some(service) => service,
                    None => {
                        return tool_error(
                            "No Store is open; call project_init or store_open first",
                        )
                    }
                };
                let method = match name {
                    "project_list_branches" => "project.list_branches",
                    "branch_fork" => "branch.fork",
                    "branch_list_commits" => "branch.list_commits",
                    "branch_merge" => "branch.merge",
                    "branch_publish" => "branch.publish",
                    "branch_diff_branches" => "branch.diff_branches",
                    "fs_read" => "fs.read",
                    "fs_list" => "fs.list",
                    "fs_stat" => "fs.stat",
                    "fs_write" => "fs.write",
                    "fs_glob" => "fs.glob",
                    "fs_grep" => "fs.grep",
                    other => return tool_error(&format!("unknown tool {other}")),
                };
                let mut args = args.clone();
                if let (Some(project), Some(object)) = (self.project.as_ref(), args.as_object_mut())
                {
                    object.insert("project".to_owned(), json!(project));
                }
                let call = SdkCall {
                    method: method.into(),
                    args: to_sdk(&args),
                };
                match layerfs_api::call(service, &call) {
                    Ok(value) => tool_text(sdk_to_json(&value)),
                    Err(refusal) => tool_error(&refusal_text(&refusal)),
                }
            }
        }
    }

    fn project_init(&mut self, args: &Json) -> Result<Json, String> {
        let source = PathBuf::from(text_arg(args, "source_path")?);
        if !source.is_dir() {
            return Err(format!(
                "source_path is not a directory: {}",
                source.display()
            ));
        }
        let name = args
            .get("name")
            .and_then(Json::as_str)
            .unwrap_or("peri-demo")
            .to_owned();
        let home = home_arg(args)?;
        ensure_private_dir(&home)?;
        let store = store_arg(args, &home, &name)?;
        if let Some(parent) = store.parent() {
            ensure_private_dir(parent)?;
        }
        let directory = home.join(&name);
        std::fs::create_dir(&directory)
            .map_err(|error| format!("project directory {}: {error}", directory.display()))?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;

        let history = |value: &str| {
            HistoryName::new(value).map_err(|error| format!("history name {value}: {error}"))
        };
        let request = InitRequest {
            source,
            store: PersistenceConfig::sqlite(directory.join("sealed.sqlite"))
                .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
            locator: LOCATOR.into(),
            policy: StoragePolicy::frozen_default(),
            catalog: HistoryCatalogConfig {
                binding_key: random()?.to_vec(),
                cursor_key: random()?,
                incarnation: 1,
            },
            stack: LayerStackId::from_authority(sixteen(random()?)),
            stack_name: history(&name)?,
            branch: BranchId::from_authority(sixteen(random()?)),
            branch_name: history("main")?,
            scope_seed: random()?,
            deadline: Instant::now() + INIT_DEADLINE,
        };
        let (sealed, _report) = Timing::disabled("project.init", |timer| {
            ProjectApi::new().init(request, timer)
        });
        let sealed = sealed.map_err(|failure| format!("project init: {failure}"))?;

        let installed = install::install_local(&sealed.store.path, &sealed.manifest, &store)
            .map_err(|failure| format!("install: {failure}"))?;
        let record = SdkValue::map([
            (
                "manifest",
                SdkValue::Bytes(
                    sealed
                        .manifest
                        .encode()
                        .map_err(|error| error.to_string())?,
                ),
            ),
            (
                "installed",
                SdkValue::Bytes(installed.encode().map_err(|error| error.to_string())?),
            ),
        ]);
        let answer = Answer {
            id: 1,
            reply: Reply::Sdk(record),
        };
        let bytes = answer.encode().map_err(|error| error.to_string())?;
        std::fs::write(directory.join("project.record"), bytes)
            .map_err(|error| format!("project record write: {error}"))?;

        let template = overlay_template()?;
        let service =
            StoreService::open_direct(&store, &installed.binding, installed.cursor_key, &template)
                .map_err(|error| format!("Store open: {error:?}"))?;
        self.service = Some(service);
        self.project = Some(name.clone());
        self.store = Some(store.clone());
        Ok(json!({
            "project": name,
            "branch": "main",
            "store": store.display().to_string()
        }))
    }

    fn store_open(&mut self, args: &Json) -> Result<Json, String> {
        let name = text_arg(args, "project")?;
        let home = home_arg(args)?;
        let store = store_arg(args, &home, name)?;
        self.open_record(name, &home, &store)?;
        Ok(json!({
            "project": name,
            "store": store.display().to_string()
        }))
    }
}

fn text_arg<'a>(args: &'a Json, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Json::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn home_arg(args: &Json) -> Result<PathBuf, String> {
    let home = args
        .get("home")
        .and_then(Json::as_str)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LAYERFS_HOME").map(PathBuf::from))
        .unwrap_or_else(|| std::env::temp_dir().join("layerfs-peri"));
    if !home.is_absolute() {
        return Err("home must be absolute".into());
    }
    Ok(home)
}

fn store_arg(args: &Json, home: &Path, name: &str) -> Result<PathBuf, String> {
    let store = args
        .get("store_path")
        .and_then(Json::as_str)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LAYERFS_STORE_PATH").map(PathBuf::from))
        .unwrap_or_else(|| home.join(format!("{name}.sqlite")));
    if !store.is_absolute() {
        return Err("store_path must be absolute".into());
    }
    Ok(store)
}

fn ensure_private_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| format!("{}: {error}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("{}: {error}", path.display()))
}

fn overlay_template() -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let directory =
        std::env::temp_dir().join(format!("layerfs-mcp-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("overlay template dir: {error}"))?;
    Ok(directory.join("overlay.sqlite"))
}

fn random() -> Result<[u8; 32], String> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .map_err(|error| error.to_string())?
        .read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn sixteen(bytes: [u8; 32]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out.copy_from_slice(&bytes[..16]);
    out
}

fn to_sdk(value: &Json) -> SdkValue {
    match value {
        Json::Null => SdkValue::Null,
        Json::Bool(value) => SdkValue::Bool(*value),
        Json::Number(value) => value
            .as_i64()
            .map(SdkValue::Int)
            .unwrap_or_else(|| SdkValue::Text(value.to_string())),
        Json::String(value) => SdkValue::Text(value.clone()),
        Json::Array(values) => SdkValue::List(values.iter().map(to_sdk).collect()),
        Json::Object(values) => SdkValue::Map(
            values
                .iter()
                .map(|(name, value)| (name.clone(), to_sdk(value)))
                .collect(),
        ),
    }
}

fn sdk_to_json(value: &SdkValue) -> Json {
    match value {
        SdkValue::Null => Json::Null,
        SdkValue::Bool(value) => json!(value),
        SdkValue::Int(value) => json!(value),
        SdkValue::Text(value) => json!(value),
        SdkValue::Bytes(value) => json!({ "bytes": hex(value) }),
        SdkValue::List(values) => Json::Array(values.iter().map(sdk_to_json).collect()),
        SdkValue::Map(fields) => {
            let mut object = serde_json::Map::new();
            for (name, value) in fields {
                object.insert(name.clone(), sdk_to_json(value));
            }
            Json::Object(object)
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn refusal_text(refusal: &SdkRefusal) -> String {
    format!("{}: {}", refusal.code, refusal.message)
}

fn tool_text(value: Json) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(value.to_string())])
}

fn tool_error(message: &str) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

fn tools() -> Vec<Tool> {
    let string = || json!({ "type": "string" });
    let integer = || json!({ "type": "integer", "minimum": 0 });
    vec![
        tool(
            "project_init",
            "Import a source directory into a new sealed LayerFS Store and open it.",
            json!({
                "source_path": string(),
                "name": string(),
                "home": string(),
                "store_path": string()
            }),
            &["source_path"],
        ),
        tool(
            "store_open",
            "Open an already initialized project record.",
            json!({ "project": string(), "home": string(), "store_path": string() }),
            &["project"],
        ),
        tool(
            "project_list_branches",
            "List branches, lowest name first.",
            json!({ "pageSize": integer(), "pagePos": string() }),
            &[],
        ),
        tool(
            "branch_fork",
            "Fork a branch from another branch, commit or layer.",
            json!({ "name": string(), "from": string() }),
            &["name"],
        ),
        tool(
            "branch_list_commits",
            "List commits on a branch.",
            json!({ "branch": string(), "pageSize": integer(), "pagePos": string() }),
            &["branch"],
        ),
        tool(
            "branch_merge",
            "Merge two branches into a new branch.",
            json!({ "branch": string(), "with": string(), "name": string(), "message": string() }),
            &["branch", "with", "name"],
        ),
        tool(
            "branch_publish",
            "Publish a branch into the shared project Store.",
            json!({ "branch": string(), "commit": string() }),
            &["branch"],
        ),
        tool(
            "branch_diff_branches",
            "Diff two branches.",
            json!({ "from": string(), "to": string() }),
            &["from", "to"],
        ),
        tool(
            "fs_read",
            "Read one window of a file at a branch, commit or layer.",
            json!({
                "branch": string(),
                "commit": string(),
                "path": string(),
                "startLine": integer(),
                "endLine": integer()
            }),
            &["path"],
        ),
        tool(
            "fs_list",
            "List one page of a directory.",
            json!({
                "branch": string(),
                "commit": string(),
                "path": string(),
                "pageSize": integer(),
                "pagePos": string()
            }),
            &[],
        ),
        tool(
            "fs_stat",
            "Stat one path.",
            json!({ "branch": string(), "commit": string(), "path": string() }),
            &["path"],
        ),
        tool(
            "fs_write",
            "Write one file on a branch and publish one commit.",
            json!({
                "branch": string(),
                "path": string(),
                "content": string(),
                "message": string()
            }),
            &["branch", "path", "content"],
        ),
        tool(
            "fs_glob",
            "Glob paths at a branch or commit.",
            json!({ "branch": string(), "commit": string(), "pattern": string() }),
            &["pattern"],
        ),
        tool(
            "fs_grep",
            "Grep file contents at a branch or commit.",
            json!({ "branch": string(), "commit": string(), "pattern": string(), "path": string() }),
            &["pattern"],
        ),
    ]
}

fn tool(
    name: &'static str,
    description: &'static str,
    properties: Json,
    required: &[&str],
) -> Tool {
    let schema = json!({
        "type": "object",
        "properties": properties,
        "required": required
    });
    let schema: JsonObject = schema
        .as_object()
        .expect("tool input schema is an object")
        .clone();
    Tool::new(name, description, schema)
}
