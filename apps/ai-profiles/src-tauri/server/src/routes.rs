//! The HTTP API. See `ai_profiles_core::api` for the shapes.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use ai_profiles_core::api::{
    ArchiveResult, ArchivedSession, CreateAccountRequest, DeleteArchiveResult, DeletedAccount,
    DirListing, HostInfo, LaunchResult, LoginCodeRequest, LoginStart, LogoutRequest, LogoutResult,
    MemoryMergeRequest, MemoryMergeResult, MoveProgress, NewSessionRequest, PairRequest,
    PairResponse, Ping, RemoteAccount, RemoteSession, RenameAccountRequest, RenameSessionRequest,
    RenameSessionResult, RestoreResult, ResumeRequest, StopResult, TransferPlan, TransferQuery,
    TransferReport, TransferRequest, WindowKey, WindowKeysRequest, WindowScreen, API_HEADER,
    API_VERSION,
};
use ai_profiles_core::transcript::{read_transcript, transcripts};
use axum::extract::{DefaultBodyLimit, Path as UrlPath, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;

use ai_profiles_core::registry::RegistryEntry;

use crate::accounts;
use crate::config::Config;
use crate::dirs;
use crate::error::ApiError;
use crate::hostinfo::{self, host_info};
use crate::launch::{self, Launch};
use crate::limits::{Lockout, RateLimit};
use crate::login::{self, LoginError, Logins};
use crate::moves;
use crate::procs::ProcessTable;
use crate::sessions::{self, TranscriptCache};
use crate::store::{Client, Store};
use crate::tmux::{self, Tmux};

/// Everything a request can reach.
pub struct ServerState {
    pub config: Config,
    pub store: Store,
    pub processes: Arc<dyn ProcessTable>,
    transcripts: TranscriptCache,
    pairing_overall: RateLimit<()>,
    pairing_per_peer: RateLimit<IpAddr>,
    bad_tokens: Lockout<IpAddr>,
    pub(crate) tmux: Tmux,
    /// One at a time per account: starting, resuming (and later deleting)
    /// look at what's running and then change it.
    account_locks: std::sync::Mutex<std::collections::HashMap<String, Arc<std::sync::Mutex<()>>>>,
    logins: Logins,
    /// The moves running now, by the id their client chose, so it can show
    /// how far each has got.
    moves: std::sync::Mutex<std::collections::HashMap<String, MoveProgress>>,
    pub(crate) state_dir: std::path::PathBuf,
    /// The installed `claude`'s version, with when it was asked.
    installed_claude: std::sync::Mutex<Option<(std::time::Instant, Option<String>)>>,
}

impl ServerState {
    pub fn new(config: Config, state_dir: &Path, processes: Arc<dyn ProcessTable>) -> ServerState {
        let minute = Duration::from_secs(60);
        ServerState {
            store: Store::new(state_dir),
            processes,
            transcripts: TranscriptCache::default(),
            pairing_overall: RateLimit::new(5, minute),
            pairing_per_peer: RateLimit::new(3, minute),
            bad_tokens: Lockout::new(10, minute, Duration::from_secs(300)),
            tmux: Tmux::new(&config.tmux_session, config.tmux_socket.as_deref()),
            account_locks: Default::default(),
            logins: Logins::default(),
            moves: Default::default(),
            state_dir: state_dir.to_path_buf(),
            installed_claude: Default::default(),
            config,
        }
    }

    pub(crate) fn account_lock(&self, account: &str) -> Arc<std::sync::Mutex<()>> {
        self.account_locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(account.to_owned())
            .or_default()
            .clone()
    }
}

/// The address a request came from, put on it by the connection loop.
#[derive(Debug, Clone, Copy)]
pub struct PeerAddr(pub SocketAddr);

fn peer_ip(request: &Request) -> IpAddr {
    request
        .extensions()
        .get::<PeerAddr>()
        .map(|peer| peer.0.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

type Shared = Arc<ServerState>;

pub fn router(state: Shared) -> Router {
    let authenticated = Router::new()
        .route("/v1/info", get(info))
        .route("/v1/accounts", get(list_accounts).post(create_account))
        .route("/v1/accounts/{name}", delete(delete_account))
        .route("/v1/accounts/{name}/login", post(start_login))
        .route("/v1/accounts/{name}/logout", post(logout))
        .route("/v1/accounts/{name}/rename", post(rename_account))
        .route("/v1/logins/{id}/code", post(submit_login))
        .route("/v1/logins/{id}", delete(cancel_login))
        .route(
            "/v1/accounts/{name}/sessions",
            get(list_sessions).post(new_session),
        )
        .route(
            "/v1/accounts/{name}/sessions/{id}/resume",
            post(resume_session),
        )
        .route("/v1/accounts/{name}/sessions/{id}/stop", post(stop_session))
        .route(
            "/v1/accounts/{name}/sessions/{id}/rename",
            post(rename_session),
        )
        .route(
            "/v1/accounts/{name}/sessions/{id}/transfer",
            // A merge Claude wrote travels in the request.
            get(plan_transfer)
                .post(transfer_session)
                .layer(DefaultBodyLimit::max(1024 * 1024)),
        )
        .route("/v1/moves/{progress_id}", get(move_progress))
        .route(
            "/v1/accounts/{name}/sessions/{id}/transfer/merge-memory",
            post(merge_memory),
        )
        .route(
            "/v1/accounts/{name}/sessions/{id}/archive",
            post(archive_session),
        )
        .route("/v1/accounts/{name}/archived", get(list_archived))
        .route(
            "/v1/accounts/{name}/archived/{id}/{archive}/restore",
            post(restore_session),
        )
        .route(
            "/v1/accounts/{name}/archived/{id}/{archive}",
            delete(delete_archive),
        )
        .route(
            "/v1/accounts/{name}/sessions/{id}/restart",
            post(restart_session),
        )
        .route(
            "/v1/accounts/{name}/windows/{window}/screen",
            get(window_screen),
        )
        .route(
            "/v1/accounts/{name}/windows/{window}/keys",
            post(window_keys),
        )
        .route("/v1/fs/dirs", get(list_dirs))
        .route("/v1/clients/self", delete(unpair))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_token));
    Router::new()
        .route("/v1/ping", get(ping))
        .route("/v1/pair", post(pair))
        .merge(authenticated)
        .fallback(|| async { ApiError::not_found("No such route.") })
        .layer(middleware::from_fn(every_request))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .with_state(state)
}

/// Refuse browsers, and say which API version answered.
///
/// Nothing a browser page sends is legitimate here: the client is a native
/// app. A request with an `Origin` header came from a page, possibly one the
/// user visited on the same network, so it is refused before anything else.
async fn every_request(request: Request, next: Next) -> Response {
    let mut response = if request.headers().contains_key(header::ORIGIN) {
        ApiError::forbidden("browser_refused", "This API doesn't answer web pages.").into_response()
    } else {
        next.run(request).await
    };
    response
        .headers_mut()
        .insert(API_HEADER, HeaderValue::from(API_VERSION));
    response
}

async fn require_token(State(state): State<Shared>, mut request: Request, next: Next) -> Response {
    let ip = peer_ip(&request);
    if state.bad_tokens.is_locked(&ip) {
        return ApiError::rate_limited().into_response();
    }
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|token| !token.is_empty());
    let client = match token.map(|token| state.store.client_for_token(token)) {
        Some(Ok(Some(client))) => client,
        Some(Err(error)) => return ApiError::from(error).into_response(),
        _ => {
            state.bad_tokens.fail(&ip);
            return ApiError::unauthorized().into_response();
        }
    };
    request.extensions_mut().insert(client);
    next.run(request).await
}

/// Run blocking work (files, subprocesses) off the async threads.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(ApiError::internal)?
}

async fn ping() -> Json<Ping> {
    Json(Ping {
        api_version: API_VERSION,
    })
}

async fn pair(
    State(state): State<Shared>,
    request: Request,
) -> Result<Json<PairResponse>, ApiError> {
    let ip = peer_ip(&request);
    if !state.pairing_per_peer.allow(&ip) || !state.pairing_overall.allow(&()) {
        return Err(ApiError::rate_limited());
    }
    let body = axum::body::to_bytes(request.into_body(), 16 * 1024)
        .await
        .map_err(|_| ApiError::invalid("The request body is too large."))?;
    let pairing: PairRequest = serde_json::from_slice(&body)
        .map_err(|_| ApiError::invalid("Expected {secret, clientName}."))?;
    if pairing.client_name.chars().count() > 128 {
        return Err(ApiError::invalid("The client name is too long."));
    }
    blocking(move || {
        let Some((client, token)) = state.store.redeem(&pairing.secret, &pairing.client_name)? else {
            return Err(ApiError {
                status: StatusCode::UNAUTHORIZED,
                code: "pairing_invalid",
                message: "That pairing code isn't valid any more: it expired or was already used. Run `ai-profiles-server pair` on the server for a new one.".into(),
            });
        };
        eprintln!("paired client {} ({}) from {ip}", client.name, client.id);
        Ok(Json(PairResponse {
            client_id: client.id,
            token,
            info: host_info(&state.config),
        }))
    })
    .await
}

async fn unpair(
    State(state): State<Shared>,
    Extension(client): Extension<Client>,
) -> Result<StatusCode, ApiError> {
    blocking(move || {
        state.store.revoke(&client.id)?;
        eprintln!("client {} ({}) unpaired itself", client.name, client.id);
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

async fn info(State(state): State<Shared>) -> Result<Json<HostInfo>, ApiError> {
    blocking(move || Ok(Json(host_info(&state.config)))).await
}

fn describe(state: &ServerState, account: accounts::AccountDir) -> RemoteAccount {
    let sessions = sessions::list(&account, state.processes.as_ref(), &state.transcripts);
    RemoteAccount {
        account: account.account(&state.config.home),
        signed_in: account.signed_in(),
        signed_in_until: account.signed_in_until().and_then(|millis| {
            chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis as i64)
                .map(|when| when.to_rfc3339())
        }),
        sessions: sessions.len() as u32,
        running_sessions: sessions.iter().filter(|s| s.running).count() as u32,
        config_dir: account.dir.display().to_string(),
        is_default: account.is_default,
        name: account.name,
    }
}

async fn list_accounts(State(state): State<Shared>) -> Result<Json<Vec<RemoteAccount>>, ApiError> {
    blocking(move || {
        let accounts = accounts::discover(&state.config)
            .into_iter()
            .map(|account| describe(&state, account))
            .collect();
        Ok(Json(accounts))
    })
    .await
}

async fn create_account(
    State(state): State<Shared>,
    Json(request): Json<CreateAccountRequest>,
) -> Result<(StatusCode, Json<RemoteAccount>), ApiError> {
    blocking(move || {
        let name = request.name.trim().to_owned();
        let made = accounts::create(&state.config, &name).map_err(|error| match error {
            accounts::CreateError::InvalidName => ApiError::invalid(
                "An account name starts with a letter or digit, then letters, digits, - and _ (64 at most), and isn't `default`.",
            ),
            accounts::CreateError::Taken => ApiError::conflict(
                "name_taken",
                format!("There is already an account called {name}."),
            ),
            accounts::CreateError::Io(message) => ApiError::internal(message),
        })?;
        eprintln!("created account {}", made.name);
        Ok((StatusCode::CREATED, Json(describe(&state, made))))
    })
    .await
}

/// Rename an account, stopping its running sessions first when asked to:
/// they have its folder open.
async fn rename_account(
    State(state): State<Shared>,
    UrlPath(name): UrlPath<String>,
    Json(request): Json<RenameAccountRequest>,
) -> Result<Json<RemoteAccount>, ApiError> {
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        let new_name = request.new_name.trim().to_owned();
        let lock = state.account_lock(&account.name);
        let _held = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let running: Vec<String> = sessions::running(&account, state.processes.as_ref())
            .into_keys()
            .collect();
        if new_name != account.name && !running.is_empty() {
            if !request.stop_running {
                return Err(ApiError::conflict(
                    "sessions_running",
                    format!(
                        "{} {} running with its folder open. Renaming stops {} first.",
                        running.len(),
                        if running.len() == 1 { "session is" } else { "sessions are" },
                        if running.len() == 1 { "it" } else { "them" }
                    ),
                ));
            }
            for id in &running {
                stop_held(&state, &account, id)?;
            }
        }
        let renamed = accounts::rename(&state.config, &account, &new_name).map_err(|error| match error {
            accounts::RenameError::Default => {
                ApiError::invalid("The default account is ~/.claude; it can't be renamed from here.")
            }
            accounts::RenameError::InvalidName => ApiError::invalid(
                "An account name starts with a letter or digit, then letters, digits, - and _ (64 at most), and isn't `default`.",
            ),
            accounts::RenameError::Taken => ApiError::conflict(
                "name_taken",
                format!("There is already an account called {new_name}."),
            ),
            accounts::RenameError::Io(message) => ApiError::internal(message),
        })?;
        crate::revive::remember(&state);
        eprintln!("renamed account {name} to {}", renamed.name);
        Ok(Json(describe(&state, renamed)))
    })
    .await
}

async fn delete_account(
    State(state): State<Shared>,
    UrlPath(name): UrlPath<String>,
) -> Result<Json<DeletedAccount>, ApiError> {
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        if account.is_default {
            return Err(ApiError::forbidden(
                "default_account",
                "The default account is ~/.claude, which the server doesn't delete.",
            ));
        }
        let lock = state.account_lock(&account.name);
        let _held = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let running = sessions::running(&account, state.processes.as_ref()).len();
        if running > 0 {
            return Err(ApiError::conflict(
                "account_busy",
                format!("{name} has {running} session(s) running. Close them first."),
            ));
        }
        if state.logins.in_progress(&account.name) {
            return Err(ApiError::conflict(
                "account_busy",
                format!("{name} is being signed in. Finish or cancel that first."),
            ));
        }
        let trashed = accounts::trash(&state.config, &account)?;
        eprintln!("moved account {name} to {}", trashed.display());
        Ok(Json(DeletedAccount {
            trashed_to: trashed.display().to_string(),
        }))
    })
    .await
}

fn login_error(error: LoginError) -> ApiError {
    match error {
        LoginError::Busy => ApiError::conflict(
            "login_busy",
            "Two sign-ins are already under way on the host. Finish one first.",
        ),
        LoginError::NotFound => ApiError {
            status: StatusCode::GONE,
            code: "login_expired",
            message: "That sign-in has ended (it expired or was cancelled). Sign in again.".into(),
        },
        LoginError::BadCode => ApiError::invalid("That doesn't look like a sign-in code."),
        LoginError::Failed { message, .. } => ApiError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "login_failed",
            message,
        },
    }
}

async fn start_login(
    State(state): State<Shared>,
    UrlPath(name): UrlPath<String>,
) -> Result<Json<LoginStart>, ApiError> {
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        let claude = hostinfo::claude_path(&state.config).ok_or_else(|| {
            ApiError::unavailable(
                "claude_not_found",
                "claude isn't installed where the server looks. Set claude_path in its config.toml.",
            )
        })?;
        let started = state
            .logins
            .start(&account, &claude, &login::login_cwd(&state.state_dir))
            .map_err(login_error)?;
        Ok(Json(LoginStart {
            login_id: started.id,
            url: started.url,
            expires_at: started.expires_at.to_rfc3339(),
        }))
    })
    .await
}

async fn submit_login(
    State(state): State<Shared>,
    UrlPath(id): UrlPath<String>,
    Json(request): Json<LoginCodeRequest>,
) -> Result<Json<RemoteAccount>, ApiError> {
    blocking(move || {
        let claude = hostinfo::claude_path(&state.config)
            .ok_or_else(|| ApiError::unavailable("claude_not_found", "claude isn't installed."))?;
        let config = state.config.clone();
        let account_name = state
            .logins
            .submit(&id, &request.code)
            .map_err(login_error)?;
        let account = accounts::find(&config, &account_name)
            .ok_or_else(|| ApiError::not_found("The account is gone."))?;
        if !login::signed_in(&claude, &account) {
            return Err(login_error(LoginError::Failed {
                message: "claude finished, but the account still isn't signed in. Sign in again."
                    .into(),
                retryable: true,
            }));
        }
        if let Err(err) = account.finish_setup(|| hostinfo::claude_version(&claude)) {
            eprintln!("could not mark {account_name} set up: {err}");
        }
        eprintln!("signed in account {account_name}");
        Ok(Json(describe(&state, account)))
    })
    .await
}

async fn cancel_login(State(state): State<Shared>, UrlPath(id): UrlPath<String>) -> StatusCode {
    state.logins.cancel(&id);
    StatusCode::NO_CONTENT
}

impl ServerState {
    /// The installed `claude`'s version, asked at most once a minute: an
    /// update lands under the same path, and sessions started before it run
    /// the old one until they're restarted.
    fn installed_claude_version(&self) -> Option<String> {
        let mut cached = self
            .installed_claude
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((at, version)) = cached.as_ref() {
            if at.elapsed() < Duration::from_secs(60) {
                return version.clone();
            }
        }
        let version = hostinfo::claude_path(&self.config)
            .and_then(|claude| hostinfo::claude_version(&claude));
        *cached = Some((std::time::Instant::now(), version.clone()));
        version
    }
}

/// How long after starting a session its Remote Control counts as connecting.
const RC_CONNECT_WINDOW_MS: u64 = 120_000;

async fn list_sessions(
    State(state): State<Shared>,
    UrlPath(name): UrlPath<String>,
) -> Result<Json<Vec<RemoteSession>>, ApiError> {
    blocking(move || {
        let account = accounts::find(&state.config, &name)
            .ok_or_else(|| ApiError::not_found(format!("There is no account called {name}.")))?;
        let mut listed = sessions::list(&account, state.processes.as_ref(), &state.transcripts);
        // A window the server opened for a session Claude hasn't registered
        // yet: it's asking something first, so the session is running, and
        // waiting.
        let waiting = waiting_windows(&state, &account);
        for session in &mut listed {
            if session.running {
                continue;
            }
            if let Some(window) = waiting.get(&session.id) {
                session.running = true;
                session.waiting = true;
                session.window = Some(window.clone());
            }
        }
        let ours: std::collections::HashSet<String> = state
            .tmux
            .tagged("@aip_session")
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        let live = sessions::running(&account, state.processes.as_ref());
        // Running an older Claude than the one now installed: it shows
        // "Update installed · Restart to update".
        if let Some(installed) = state.installed_claude_version() {
            for session in &mut listed {
                session.update_pending = session.running
                    && session
                        .claude_version
                        .as_deref()
                        .is_some_and(|running| sessions::newer_version(&installed, running));
            }
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as u64);
        for session in &mut listed {
            if !session.running || session.waiting {
                continue;
            }
            // Every session the server starts has Remote Control on; one that
            // started moments ago is still connecting. Past that, something
            // went wrong, and saying "connecting" would never end.
            let just_started = ours.contains(&session.id)
                && live
                    .get(&session.id)
                    .and_then(|entry| entry.started_at)
                    .is_some_and(|started| now_ms.saturating_sub(started) < RC_CONNECT_WINDOW_MS);
            let disconnected = session.window.as_ref().is_some_and(|window| {
                state
                    .tmux
                    .capture(&window.pane_id)
                    .is_some_and(|screen| launch::remote_control_disconnected(&screen))
            });
            if disconnected {
                // The registry keeps a Remote Control id after Remote Control
                // ends; the window says whether it did. But a resumed session
                // replays its old conversation, an old "disconnected" line
                // included, until it has connected again: just started, that
                // line is history.
                session.remote_control = false;
                session.bridge_session_id = None;
                session.remote_control_connecting = just_started;
                continue;
            }
            session.remote_control_connecting = just_started && !session.remote_control;
        }
        Ok(Json(listed))
    })
    .await
}

#[derive(Deserialize)]
struct DirQuery {
    path: Option<String>,
    #[serde(default)]
    hidden: bool,
}

async fn list_dirs(
    State(state): State<Shared>,
    Query(query): Query<DirQuery>,
) -> Result<Json<DirListing>, ApiError> {
    blocking(move || {
        dirs::list(
            &state.config.folder_roots,
            &state.config.home,
            query.path.as_deref(),
            query.hidden,
        )
        .map(Json)
    })
    .await
}

fn account_or_404(state: &ServerState, name: &str) -> Result<accounts::AccountDir, ApiError> {
    accounts::find(&state.config, name)
        .ok_or_else(|| ApiError::not_found(format!("There is no account called {name}.")))
}

/// What starting anything needs: `claude`, and tmux.
fn launch_tools(state: &ServerState) -> Result<std::path::PathBuf, ApiError> {
    if hostinfo::tmux_version().is_none() {
        return Err(ApiError::unavailable(
            "tmux_unavailable",
            "tmux isn't installed on the host, so sessions can't be started there.",
        ));
    }
    hostinfo::claude_path(&state.config).ok_or_else(|| {
        ApiError::unavailable(
            "claude_not_found",
            "claude isn't installed where the server looks. Set claude_path in its config.toml.",
        )
    })
}

/// A name for Remote Control and the tmux window, as the user typed it:
/// one line, printable, not too long.
fn session_name(name: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(name) = name else {
        return Ok(None);
    };
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Ok(None);
    }
    if name.chars().any(char::is_control) || name.chars().count() > 100 || name.starts_with('-') {
        return Err(ApiError::invalid(
            "A session name is one line of up to 100 characters, not starting with -.",
        ));
    }
    Ok(Some(name))
}

/// `8-4-4-4-12` hex, the only session ids Claude makes.
fn is_session_id(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit()))
}

async fn new_session(
    State(state): State<Shared>,
    UrlPath(name): UrlPath<String>,
    Json(request): Json<NewSessionRequest>,
) -> Result<Json<LaunchResult>, ApiError> {
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        let claude = launch_tools(&state)?;
        let cwd =
            dirs::resolve_inside(&state.config.folder_roots, &state.config.home, &request.cwd)?;
        let folder = cwd
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Unnamed, it's named after its folder rather than something Remote
        // Control makes up, so the Claude app, the registry and the list agree.
        let remote_control_name =
            session_name(request.name)?.or_else(|| (!folder.is_empty()).then(|| folder.clone()));
        let window_name = tmux::window_name(
            remote_control_name.as_deref().unwrap_or(""),
            &format!("{}-{folder}", account.name),
        );
        let lock = state.account_lock(&account.name);
        let _held = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let started = launch::start(
            &state.tmux,
            &Launch {
                account: &account,
                claude: &claude,
                cwd: &cwd,
                window_name,
                remote_control_name,
                resume: None,
                trust_folder: request.trust_folder,
            },
        );
        state.processes.changed(&account);
        started.map(Json)
    })
    .await
}

async fn resume_session(
    State(state): State<Shared>,
    UrlPath((name, id)): UrlPath<(String, String)>,
    Json(request): Json<ResumeRequest>,
) -> Result<Json<LaunchResult>, ApiError> {
    if !is_session_id(&id) {
        return Err(ApiError::invalid("That isn't a session id."));
    }
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        let lock = state.account_lock(&account.name);
        let _held = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        resume_held(&state, &account, &id, request.trust_folder).map(Json)
    })
    .await
}

/// End a running session. Answers whether it was running; a session that
/// wasn't is left as it is.
async fn stop_session(
    State(state): State<Shared>,
    UrlPath((name, id)): UrlPath<(String, String)>,
) -> Result<Json<StopResult>, ApiError> {
    if !is_session_id(&id) {
        return Err(ApiError::invalid("That isn't a session id."));
    }
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        let lock = state.account_lock(&account.name);
        let _held = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let was_running = stop_held(&state, &account, &id)?;
        crate::revive::remember(&state);
        Ok(Json(StopResult { was_running }))
    })
    .await
}

/// Stop a session and resume it, so it comes back on whatever `claude` is
/// installed now: how a session picks up an upgrade. One that wasn't running
/// is just resumed. It comes back in a window of its own in the server's
/// tmux session, wherever it ran before.
async fn restart_session(
    State(state): State<Shared>,
    UrlPath((name, id)): UrlPath<(String, String)>,
    Json(request): Json<ResumeRequest>,
) -> Result<Json<LaunchResult>, ApiError> {
    if !is_session_id(&id) {
        return Err(ApiError::invalid("That isn't a session id."));
    }
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        let lock = state.account_lock(&account.name);
        let _held = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        stop_held(&state, &account, &id)?;
        resume_held(&state, &account, &id, request.trust_folder).map(Json)
    })
    .await
}

/// [`sessions::stop`], with the account's lock held, then closing the window
/// if the server made it for this session: its pane ran only `claude`, and
/// tmux may be set to keep it open once that has exited.
/// The windows the server opened for `account`'s sessions that Claude hasn't
/// registered yet, by session id.
fn waiting_windows(
    state: &ServerState,
    account: &accounts::AccountDir,
) -> std::collections::HashMap<String, ai_profiles_core::api::TmuxWindow> {
    let registered = sessions::running(account, state.processes.as_ref());
    state
        .tmux
        .tagged("@aip_session")
        .into_iter()
        .filter(|(_, id)| !registered.contains_key(id))
        .filter(|(window, _)| {
            state
                .tmux
                .window(&window.window_id)
                .is_some_and(|(_, tag)| tag == account.name)
        })
        .map(|(window, id)| (id, window))
        .collect()
}

fn stop_held(
    state: &ServerState,
    account: &accounts::AccountDir,
    id: &str,
) -> Result<bool, ApiError> {
    let Some(entry) = sessions::stop(account, id, state.processes.as_ref())
        .map_err(|message| ApiError::conflict("session_did_not_stop", message))?
    else {
        // Not started yet, waiting in a window the server opened for it
        // alone: closing the window ends it.
        if let Some(window) = waiting_windows(state, account).get(id) {
            state
                .tmux
                .kill_window(&window.window_id)
                .map_err(ApiError::internal)?;
            return Ok(true);
        }
        return Ok(false);
    };
    if let Some(location) = entry.tmux_location() {
        let ours = state
            .tmux
            .tagged("@aip_session")
            .into_iter()
            .any(|(window, session)| window.window_id == location.window_id && session == id);
        if ours {
            let _ = state.tmux.kill_window(&location.window_id);
        }
    }
    Ok(true)
}

/// Resume session `id` of `account`, or find the window it's running in, with
/// the account's lock held.
pub(crate) fn resume_held(
    state: &ServerState,
    account: &accounts::AccountDir,
    id: &str,
    trust_folder: bool,
) -> Result<LaunchResult, ApiError> {
    let id = id.to_owned();
    // Already running: point at it rather than start a second copy.
    if let Some(entry) = sessions::running(account, state.processes.as_ref()).get(&id) {
        let Some(location) = entry.tmux_location() else {
            return Err(ApiError::conflict(
                "session_running_outside_tmux",
                format!(
                    "It's already open outside tmux (process {}). Close it there first.",
                    entry.pid
                ),
            ));
        };
        let window = ai_profiles_core::api::TmuxWindow {
            session: location.session,
            window_id: location.window_id,
            pane_id: location.pane_id,
        };
        return Ok(already_running(&state.tmux, window, &id));
    }
    // Just started, before Claude registered it: say what it's waiting for,
    // so it can be answered.
    if let Some((window, _)) = state
        .tmux
        .tagged("@aip_session")
        .into_iter()
        .find(|(_, session)| *session == id)
    {
        let attention = launch::attention_now(&state.tmux, &window.pane_id);
        return Ok(LaunchResult {
            attention,
            ..already_running(&state.tmux, window, &id)
        });
    }

    let (_, _, transcript) = transcripts(&account.dir)
        .into_iter()
        .find(|(_, session, _)| *session == id)
        .ok_or_else(|| ApiError::not_found("There is no session with that id."))?;
    let claude = launch_tools(state)?;
    let info = read_transcript(&transcript)?;
    let cwd = info
        .cwd
        .as_deref()
        .map(std::path::PathBuf::from)
        .filter(|cwd| cwd.is_dir())
        .ok_or_else(|| {
            ApiError::conflict(
                "folder_missing",
                format!(
                    "The folder it worked in ({}) isn't there any more.",
                    info.cwd.as_deref().unwrap_or("unknown")
                ),
            )
        })?;
    let window_name = tmux::window_name(
        info.custom_title
            .as_deref()
            .or(info.ai_title.as_deref())
            .unwrap_or(""),
        &format!("claude-{}", &id[..8]),
    );
    let started = launch::start(
        &state.tmux,
        &Launch {
            account,
            claude: &claude,
            cwd: &cwd,
            window_name,
            // Named after what the app lists it as, generated title included,
            // else its folder, so the Claude app, the registry and the list
            // agree, rather than something Remote Control makes up.
            remote_control_name: info.title().or_else(|| {
                cwd.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            }),
            resume: Some(id.clone()),
            trust_folder,
        },
    );
    state.processes.changed(account);
    started
}

/// `%Y%m%d-%H%M%S` in the host's local time, as claudemulti names backups.
fn stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

async fn plan_transfer(
    State(state): State<Shared>,
    UrlPath((name, id)): UrlPath<(String, String)>,
    Query(query): Query<TransferQuery>,
) -> Result<Json<TransferPlan>, ApiError> {
    if !is_session_id(&id) {
        return Err(ApiError::invalid("That isn't a session id."));
    }
    blocking(move || {
        let source = account_or_404(&state, &name)?;
        let destination = account_or_404(&state, &query.to)?;
        moves::plan(
            &state.config,
            state.processes.as_ref(),
            source,
            &id,
            destination,
        )
        .map(|prepared| Json(prepared.plan))
    })
    .await
}

/// Move a session to another account on the host, then, if asked, archive
/// the source's copy and resume it under the destination.
async fn transfer_session(
    State(state): State<Shared>,
    UrlPath((name, id)): UrlPath<(String, String)>,
    Json(request): Json<TransferRequest>,
) -> Result<Json<TransferReport>, ApiError> {
    if !is_session_id(&id) {
        return Err(ApiError::invalid("That isn't a session id."));
    }
    if request
        .memory
        .keys()
        .any(|path| !moves::is_memory_path(path))
    {
        return Err(ApiError::invalid(
            "A memory path names a file outside the memory folder.",
        ));
    }
    let afterwards = match (request.archive_source, request.delete_source) {
        (true, true) => {
            return Err(ApiError::invalid(
                "Archive the copy left behind, or delete it: not both.",
            ))
        }
        (true, false) => moves::Afterwards::Archive,
        (false, true) => moves::Afterwards::Delete,
        (false, false) => moves::Afterwards::Keep,
    };
    let progress = request
        .progress_id
        .clone()
        .filter(|id| is_session_id(id))
        .map(|id| MoveTracker::new(state.clone(), id));
    blocking(move || {
        let source = account_or_404(&state, &name)?;
        let destination = account_or_404(&state, &request.to)?;
        let stopping = request
            .stop_first
            .then(|| "Stopping the session".to_owned());
        let copying = format!("Copying it to {}", destination.name);
        let starting = request
            .resume
            .then(|| format!("Starting it in {}", destination.name));
        if let Some(progress) = &progress {
            progress.plan(
                [stopping.clone(), Some(copying.clone()), starting.clone()]
                    .into_iter()
                    .flatten()
                    .collect(),
            );
        }
        let step = |name: &Option<String>| {
            if let (Some(progress), Some(name)) = (&progress, name) {
                progress.at(name);
            }
        };
        // Both accounts change: take their locks in one order, so two moves
        // the other way round can't wait on each other.
        let mut names = [source.name.clone(), destination.name.clone()];
        names.sort();
        let locks: Vec<_> = names.iter().map(|name| state.account_lock(name)).collect();
        let _held: Vec<_> = locks
            .iter()
            .map(|lock| {
                lock.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
            })
            .collect();
        let mut prepared = moves::plan(
            &state.config,
            state.processes.as_ref(),
            source,
            &id,
            destination,
        )?;
        if request.stop_first && prepared.plan.running.iter().any(|found| found.exact) {
            step(&stopping);
            for found in prepared.plan.running.iter().filter(|found| found.exact) {
                if let Some(account) = accounts::find(&state.config, &found.account) {
                    stop_held(&state, &account, &id)?;
                }
            }
            crate::revive::remember(&state);
            // What's running has changed: plan again.
            prepared = moves::plan(
                &state.config,
                state.processes.as_ref(),
                prepared.source,
                &id,
                prepared.destination,
            )?;
        }
        step(&Some(copying));
        let moved = moves::transfer(
            &prepared,
            request.replace_newer,
            request.confirm_running,
            &request.memory,
            afterwards,
            &stamp(),
        )?;
        let (launch, resume_error) = if request.resume {
            step(&starting);
            match resume_held(&state, &prepared.destination, &id, request.trust_folder) {
                Ok(launch) => (Some(launch), None),
                Err(err) => (None, Some(err.message)),
            }
        } else {
            (None, None)
        };
        Ok(Json(TransferReport {
            changed: moved.changed,
            backup_dir: moved.backup_dir,
            memory: moved.memory,
            archived_to: moved.archived_to,
            freed_bytes: moved.freed_bytes,
            delete_error: moved.delete_error,
            launch,
            resume_error,
        }))
    })
    .await
}

async fn rename_session(
    State(state): State<Shared>,
    UrlPath((name, id)): UrlPath<(String, String)>,
    Json(request): Json<RenameSessionRequest>,
) -> Result<Json<RenameSessionResult>, ApiError> {
    if !is_session_id(&id) {
        return Err(ApiError::invalid("That isn't a session id."));
    }
    let Some(new_name) = crate::rename::valid_name(&request.name) else {
        return Err(ApiError::invalid(format!(
            "A session's name is one line of up to {} characters, not starting with -.",
            crate::rename::MAX_NAME
        )));
    };
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        let lock = state.account_lock(&account.name);
        let _held = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(entry) = sessions::running(&account, state.processes.as_ref()).remove(&id) {
            // Only in a window the server opened: anywhere else, the keys
            // could land in someone's shell.
            let window = state
                .tmux
                .tagged("@aip_session")
                .into_iter()
                .find(|(window, session)| {
                    *session == id
                        && state
                            .tmux
                            .window(&window.window_id)
                            .is_some_and(|(_, tag)| tag == account.name)
                })
                .map(|(window, _)| window);
            let Some(window) = window else {
                return Err(ApiError::conflict(
                    "session_running_elsewhere",
                    "It's running in a window ai-profiles didn't open. Rename it there with /rename, or in the Claude app.",
                ));
            };
            let registry = account
                .dir
                .join("sessions")
                .join(format!("{}.json", entry.pid));
            let settled = crate::rename::rename_live(
                &state.tmux,
                &window.pane_id,
                &registry,
                &entry,
                &new_name,
            )?;
            return Ok(Json(RenameSessionResult {
                name: settled,
                live: true,
            }));
        }
        if waiting_windows(&state, &account).contains_key(&id) {
            return Err(ApiError::conflict(
                "session_waiting",
                "It's waiting for an answer before it starts. Answer it, then rename it.",
            ));
        }
        let transcript = moves::transcript_of(&account, &id)?;
        crate::rename::rename_stopped(&transcript, &id, &new_name).map_err(ApiError::internal)?;
        Ok(Json(RenameSessionResult {
            name: new_name,
            live: false,
        }))
    })
    .await
}

/// A move's progress, kept while it runs and dropped when it ends.
struct MoveTracker {
    state: Shared,
    id: String,
}

impl MoveTracker {
    fn new(state: Shared, id: String) -> MoveTracker {
        MoveTracker { state, id }
    }

    fn moves(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, MoveProgress>> {
        self.state
            .moves
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn plan(&self, steps: Vec<String>) {
        self.moves()
            .insert(self.id.clone(), MoveProgress { steps, current: 0 });
    }

    fn at(&self, step: &str) {
        if let Some(progress) = self.moves().get_mut(&self.id) {
            if let Some(index) = progress.steps.iter().position(|name| name == step) {
                progress.current = index;
            }
        }
    }
}

impl Drop for MoveTracker {
    fn drop(&mut self) {
        let id = self.id.clone();
        self.moves().remove(&id);
    }
}

async fn move_progress(
    State(state): State<Shared>,
    UrlPath(progress_id): UrlPath<String>,
) -> Result<Json<MoveProgress>, ApiError> {
    state
        .moves
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&progress_id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("No move is running with that id."))
}

async fn merge_memory(
    State(state): State<Shared>,
    UrlPath((name, id)): UrlPath<(String, String)>,
    Json(request): Json<MemoryMergeRequest>,
) -> Result<Json<MemoryMergeResult>, ApiError> {
    if !is_session_id(&id) {
        return Err(ApiError::invalid("That isn't a session id."));
    }
    if !moves::is_memory_path(&request.path) {
        return Err(ApiError::invalid("That isn't a memory file."));
    }
    blocking(move || {
        let source = account_or_404(&state, &name)?;
        let destination = account_or_404(&state, &request.to)?;
        let prepared = moves::plan(
            &state.config,
            state.processes.as_ref(),
            source,
            &id,
            destination,
        )?;
        moves::claude_merge(&state.config, &state.state_dir, &prepared, &request.path)
            .map(|merged| Json(MemoryMergeResult { merged }))
    })
    .await
}

async fn archive_session(
    State(state): State<Shared>,
    UrlPath((name, id)): UrlPath<(String, String)>,
) -> Result<Json<ArchiveResult>, ApiError> {
    if !is_session_id(&id) {
        return Err(ApiError::invalid("That isn't a session id."));
    }
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        let lock = state.account_lock(&account.name);
        let _held = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let archived = moves::archive(
            &state.config,
            state.processes.as_ref(),
            &account,
            &id,
            &stamp(),
        )?;
        Ok(Json(ArchiveResult {
            archived_to: archived.display().to_string(),
        }))
    })
    .await
}

async fn list_archived(
    State(state): State<Shared>,
    UrlPath(name): UrlPath<String>,
) -> Result<Json<Vec<ArchivedSession>>, ApiError> {
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        Ok(Json(moves::archived(&account, &state.transcripts)))
    })
    .await
}

async fn restore_session(
    State(state): State<Shared>,
    UrlPath((name, id, archive)): UrlPath<(String, String, String)>,
) -> Result<Json<RestoreResult>, ApiError> {
    if !is_session_id(&id) || !archive.ends_with("-archived") || !moves::is_memory_path(&archive) {
        return Err(ApiError::invalid("That isn't an archived session."));
    }
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        let lock = state.account_lock(&account.name);
        let _held = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let transcript = moves::restore(&account, &id, &archive)?;
        Ok(Json(RestoreResult {
            transcript: transcript.display().to_string(),
        }))
    })
    .await
}

async fn delete_archive(
    State(state): State<Shared>,
    UrlPath((name, id, archive)): UrlPath<(String, String, String)>,
) -> Result<Json<DeleteArchiveResult>, ApiError> {
    if !is_session_id(&id) || !archive.ends_with("-archived") || !moves::is_memory_path(&archive) {
        return Err(ApiError::invalid("That isn't an archived session."));
    }
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        let lock = state.account_lock(&account.name);
        let _held = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let freed_bytes = moves::delete_archive(&account, &id, &archive)?;
        Ok(Json(DeleteArchiveResult { freed_bytes }))
    })
    .await
}

/// Sign an account out with `claude auth logout`. Its running sessions are
/// stopped first when asked, and refused with `sessions_running` otherwise:
/// they would go on until their token next renews, then fail.
async fn logout(
    State(state): State<Shared>,
    UrlPath(name): UrlPath<String>,
    Json(request): Json<LogoutRequest>,
) -> Result<Json<LogoutResult>, ApiError> {
    blocking(move || {
        let account = account_or_404(&state, &name)?;
        let claude = launch_tools(&state)?;
        let lock = state.account_lock(&account.name);
        let _held = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let running: Vec<String> = sessions::running(&account, state.processes.as_ref())
            .into_keys()
            .collect();
        if !running.is_empty() && !request.stop_running {
            return Err(ApiError::conflict(
                "sessions_running",
                format!(
                    "{} {} running. Signing out stops {}.",
                    running.len(),
                    if running.len() == 1 {
                        "session is"
                    } else {
                        "sessions are"
                    },
                    if running.len() == 1 { "it" } else { "them" }
                ),
            ));
        }
        let mut stopped = 0;
        for id in &running {
            if stop_held(&state, &account, id)? {
                stopped += 1;
            }
        }
        crate::revive::remember(&state);
        let mut command = std::process::Command::new(&claude);
        command
            .args(["auth", "logout"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        if account.is_default {
            command.env_remove("CLAUDE_CONFIG_DIR");
        } else {
            command.env("CLAUDE_CONFIG_DIR", &account.dir);
        }
        let output = command.output().map_err(ApiError::internal)?;
        if !output.status.success() || account.signed_in() {
            return Err(ApiError::conflict(
                "logout_failed",
                format!(
                    "claude auth logout didn't sign it out: {}",
                    String::from_utf8_lossy(&output.stderr)
                        .lines()
                        .next()
                        .unwrap_or("no reason given")
                ),
            ));
        }
        Ok(Json(LogoutResult { stopped }))
    })
    .await
}

fn already_running(
    tmux: &Tmux,
    window: ai_profiles_core::api::TmuxWindow,
    id: &str,
) -> LaunchResult {
    LaunchResult {
        attach_command: tmux.attach_command(&window),
        window,
        already_running: true,
        session_id: Some(id.to_owned()),
        remote_control_name: None,
        attention: None,
    }
}

/// Keys the app may press in a window, by tmux's names: enough to answer
/// any prompt and to stop Claude, nothing that drives tmux itself.
const WINDOW_KEYS: &[&str] = &[
    "Enter", "Escape", "Tab", "BTab", "BSpace", "Space", "Up", "Down", "Left", "Right", "Home",
    "End", "PageUp", "PageDown", "C-c", "C-d",
];
const MAX_TYPED: usize = 2000;

/// A window this server opened for `account`. Anything else (the user's
/// own tmux windows, another account's) is "not found".
fn our_window(
    state: &ServerState,
    account: &str,
    window: &str,
) -> Result<(tmux::Launched, accounts::AccountDir), ApiError> {
    let plain = window.len() > 1
        && window.len() <= 10
        && window.starts_with('@')
        && window[1..].bytes().all(|b| b.is_ascii_digit());
    if !plain {
        return Err(ApiError::invalid("That isn't a tmux window id."));
    }
    let account = account_or_404(state, account)?;
    match state.tmux.window(window) {
        Some((found, tag)) if tag == account.name => Ok((found, account)),
        _ => Err(window_gone()),
    }
}

fn window_gone() -> ApiError {
    ApiError {
        status: StatusCode::NOT_FOUND,
        code: "window_gone",
        message: "That window has closed.".into(),
    }
}

/// What the window shows, and whether Claude in it has registered its
/// session (its pane runs claude itself, so the registry file is named for
/// the pane's pid).
fn screen_of(
    state: &ServerState,
    (window, account): &(tmux::Launched, accounts::AccountDir),
) -> Result<WindowScreen, ApiError> {
    let (text, width, height) = state
        .tmux
        .screen(&window.window.pane_id)
        .ok_or_else(window_gone)?;
    let registered = std::fs::read_to_string(
        account
            .dir
            .join("sessions")
            .join(format!("{}.json", window.pane_pid)),
    )
    .ok()
    .and_then(|text| serde_json::from_str::<RegistryEntry>(&text).ok());
    Ok(WindowScreen {
        text,
        width,
        height,
        remote_control: registered
            .as_ref()
            .is_some_and(|entry| entry.bridge_session_id.is_some()),
        session_id: registered.and_then(|entry| entry.session_id),
    })
}

async fn window_screen(
    State(state): State<Shared>,
    UrlPath((name, window)): UrlPath<(String, String)>,
) -> Result<Json<WindowScreen>, ApiError> {
    blocking(move || {
        let window = our_window(&state, &name, &window)?;
        screen_of(&state, &window).map(Json)
    })
    .await
}

/// Type into a window the server opened, then show what it looks like.
/// For getting a session past whatever Claude asks when it starts, from
/// the app, without a terminal.
async fn window_keys(
    State(state): State<Shared>,
    UrlPath((name, window)): UrlPath<(String, String)>,
    Json(request): Json<WindowKeysRequest>,
) -> Result<Json<WindowScreen>, ApiError> {
    if request.keys.is_empty() || request.keys.len() > 50 {
        return Err(ApiError::invalid("Send between 1 and 50 keys at a time."));
    }
    for key in &request.keys {
        let fine = match key {
            WindowKey::Key(name) => WINDOW_KEYS.contains(&name.as_str()),
            WindowKey::Text(text) => {
                !text.is_empty()
                    && text.chars().count() <= MAX_TYPED
                    && !text.chars().any(char::is_control)
            }
        };
        if !fine {
            return Err(ApiError::invalid("That key can't be sent."));
        }
    }
    blocking(move || {
        let window = our_window(&state, &name, &window)?;
        let pane = &window.0.window.pane_id;
        for key in &request.keys {
            match key {
                WindowKey::Key(name) => state.tmux.press(pane, name),
                WindowKey::Text(text) => state.tmux.type_text(pane, text),
            }
            .map_err(ApiError::internal)?;
        }
        // Let Claude redraw before looking.
        std::thread::sleep(std::time::Duration::from_millis(150));
        screen_of(&state, &window).map(Json)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_claude_style_session_ids() {
        assert!(is_session_id("5d0f1a2b-3c4d-4e5f-8a9b-0c1d2e3f4a5b"));
        assert!(!is_session_id("5d0f1a2b"));
        assert!(!is_session_id("../../etc-passwd-xx-xxxx-xxxxxxxxxxxx"));
        assert!(!is_session_id("5d0f1a2b-3c4d-4e5f-8a9b-0c1d2e3f4a5g"));
    }

    #[test]
    fn session_names_are_one_tidy_line() {
        assert_eq!(session_name(None).unwrap(), None);
        assert_eq!(session_name(Some("   ".into())).unwrap(), None);
        assert_eq!(
            session_name(Some("  Billing \n fix ".into()))
                .unwrap()
                .as_deref(),
            Some("Billing fix")
        );
        assert!(session_name(Some("-rf".into())).is_err());
        assert!(session_name(Some("a\u{7}b".into())).is_err());
        assert!(session_name(Some("x".repeat(101))).is_err());
    }
}
