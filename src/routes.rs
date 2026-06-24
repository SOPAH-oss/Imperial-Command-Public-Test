use crate::{
    bot_driver::BotController,
    casino::{self, BlackjackGame},
    models::{
        AppUser, BlackjackStartRequest, BookWriterRequest, BotConfig, ButlerTransferRequest,
        ChestLedgerConfig, LoginRequest, LoginResponse, NewBountyEntry, NewButlerChestEntry,
        NewButlerWaypointEntry, NewLedgerChestEntry, NewPearlEntry, NewShopItemEntry,
        NewUserRequest, NewWaypointEntry, RouletteRequest, SlotsRequest, UpdateBankBalanceRequest, UpdateUserRequest, UserRole,
        ViewportSnapshotRequest, VipTableEntry, WaypointEntry,
    },
    storage,
};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde_json::Value;
use std::{collections::HashMap, env, process::Command, sync::Arc};
use tokio::sync::RwLock;
use tower_http::services::ServeDir;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub bot: BotController,
    pub sessions: Arc<RwLock<HashMap<String, AppUser>>>,
    pub blackjack_sessions: Arc<RwLock<HashMap<String, BlackjackGame>>>,
}

pub fn app(bot: BotController) -> Router {
    let state = Arc::new(AppState {
        bot,
        sessions: Arc::new(RwLock::new(HashMap::new())),
        blackjack_sessions: Arc::new(RwLock::new(HashMap::new())),
    });
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .route("/api/users", get(list_users).post(add_user))
        .route("/api/users/:username", put(update_user).delete(delete_user))
        .route("/api/config", get(get_config).post(save_config))
        .route("/api/bot/status", get(bot_status))
        .route("/api/bot/connect", post(connect_bot))
        .route("/api/bot/disconnect", post(disconnect_bot))
        .route("/api/bot/hard-stop", post(hard_stop_bot))
        .route("/api/bot/halt", post(halt_bot_action))
        .route("/api/bot/accounts/:name/connect", post(connect_bot_account))
        .route(
            "/api/bot/accounts/:name/disconnect",
            post(disconnect_bot_account),
        )
        .route(
            "/api/bot/accounts/:name/hard-stop",
            post(hard_stop_bot_account),
        )
        .route("/api/bot/chat", post(bot_chat))
        .route("/api/pearls", get(list_pearls).post(add_pearl))
        .route("/api/pearls/:id", put(update_pearl).delete(delete_pearl))
        .route("/api/pearls/:id/pull", post(pull_pearl))
        .route("/api/pearls/:id/throw", post(throw_stasis))
        .route("/api/waypoints", get(list_waypoints).post(add_waypoint))
        .route(
            "/api/waypoints/:id",
            put(update_waypoint).delete(delete_waypoint),
        )
        .route("/api/waypoints/:id/walk", post(walk_to_waypoint))
        .route("/api/casino/balance", get(casino_balance))
        .route("/api/casino/roulette", post(casino_roulette))
        .route("/api/casino/slots", post(casino_slots))
        .route("/api/casino/blackjack/start", post(blackjack_start))
        .route("/api/casino/blackjack/hit", post(blackjack_hit))
        .route("/api/casino/blackjack/stand", post(blackjack_stand))
        .route("/api/bank/balances", get(bank_balances).post(set_bank_balance))
        .route("/api/vip/table", get(vip_table))
        .route("/api/shop/items", get(list_shop_items).post(add_shop_item))
        .route(
            "/api/shop/items/:id",
            put(update_shop_item).delete(delete_shop_item),
        )
        .route("/api/bounties", get(list_bounties).post(add_bounty))
        .route(
            "/api/bounties/:id",
            put(update_bounty).delete(delete_bounty),
        )
        .route("/api/chest-ledger/process", post(process_chest_ledger))
        .route(
            "/api/ledger-chests",
            get(list_ledger_chests).post(add_ledger_chest),
        )
        .route(
            "/api/ledger-chests/:id",
            put(update_ledger_chest).delete(delete_ledger_chest),
        )
        .route("/api/ledger-chests/:id/walk", post(walk_ledger_chest))
        .route("/api/ledger-chests/:id/process", post(process_ledger_chest))
        .route("/api/books/write-sign-place", post(write_sign_place_book))
        .route(
            "/api/butler/chests",
            get(list_butler_chests).post(add_butler_chest),
        )
        .route(
            "/api/butler/chests/:id",
            put(update_butler_chest).delete(delete_butler_chest),
        )
        .route("/api/butler/chests/:id/walk", post(walk_butler_chest))
        .route(
            "/api/butler/waypoints",
            get(list_butler_waypoints).post(add_butler_waypoint),
        )
        .route(
            "/api/butler/waypoints/:id",
            put(update_butler_waypoint).delete(delete_butler_waypoint),
        )
        .route("/api/butler/waypoints/:id/walk", post(walk_butler_waypoint))
        .route("/api/butler/transfer", post(butler_transfer))
        .route("/api/faq/public", get(faq_public_list))
        .route("/api/greeter/public", get(greeter_public_list))
        .route("/api/public/collections", get(public_collections))
        .route("/api/public/search", post(public_search))
        .route("/api/public/dashboard", get(public_dashboard))
        .route("/api/public/:collection", get(public_list).post(public_add))
        .route("/api/public/:collection/:id", put(public_update).delete(public_delete))
        .route("/api/viewport/snapshot", post(viewport_snapshot))
        .nest_service(
            "/",
            ServeDir::new("static").append_index_html_on_directories(true),
        )
        .with_state(state)
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let part = part.trim();
            part.strip_prefix("ps_session=").map(|s| s.to_string())
        })
}

async fn current_user(
    state: &Arc<AppState>,
    headers: &HeaderMap,
) -> Result<AppUser, (StatusCode, String)> {
    let Some(token) = cookie_token(headers) else {
        return Err((StatusCode::UNAUTHORIZED, "login required".into()));
    };
    state
        .sessions
        .read()
        .await
        .get(&token)
        .cloned()
        .ok_or((StatusCode::UNAUTHORIZED, "login expired".into()))
}

async fn owner_user(
    state: &Arc<AppState>,
    headers: &HeaderMap,
) -> Result<AppUser, (StatusCode, String)> {
    let user = current_user(state, headers).await?;
    if user.role == UserRole::Owner {
        Ok(user)
    } else {
        Err((StatusCode::FORBIDDEN, "owner login required".into()))
    }
}

async fn feature_user(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    permission: &str,
) -> Result<AppUser, (StatusCode, String)> {
    let user = current_user(state, headers).await?;
    if user.role == UserRole::Owner
        || user.permissions.iter().any(|p| p.eq_ignore_ascii_case(permission))
    {
        Ok(user)
    } else {
        Err((StatusCode::FORBIDDEN, format!("{permission} permission required")))
    }
}

async fn feature_any_user(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    permissions: &[&str],
) -> Result<AppUser, (StatusCode, String)> {
    let user = current_user(state, headers).await?;
    if user.role == UserRole::Owner
        || permissions.iter().any(|needed| {
            user.permissions.iter().any(|p| p.eq_ignore_ascii_case(needed))
        })
    {
        Ok(user)
    } else {
        Err((StatusCode::FORBIDDEN, "required page permission missing".into()))
    }
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    let Some(user) = storage::verify_user(&req.username, &req.password) else {
        return (StatusCode::UNAUTHORIZED, "invalid username or password").into_response();
    };
    let token = Uuid::new_v4().to_string();
    state
        .sessions
        .write()
        .await
        .insert(token.clone(), user.clone());
    let cookie = format!("ps_session={}; HttpOnly; SameSite=Lax; Path=/", token);
    let mut headers = HeaderMap::new();
    headers.insert("set-cookie", HeaderValue::from_str(&cookie).unwrap());
    (
        headers,
        Json(LoginResponse {
            username: user.username,
            role: user.role,
            minecraft_name: user.minecraft_name,
            discord_name: user.discord_name,
            permissions: user.permissions,
        }),
    )
        .into_response()
}

async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(token) = cookie_token(&headers) {
        state.sessions.write().await.remove(&token);
    }
    let mut out = HeaderMap::new();
    out.insert(
        "set-cookie",
        HeaderValue::from_static("ps_session=; Max-Age=0; Path=/"),
    );
    (out, "logged out").into_response()
}

async fn me(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    match current_user(&state, &headers).await {
        Ok(u) => Json(LoginResponse {
            username: u.username,
            role: u.role,
            minecraft_name: u.minecraft_name,
            discord_name: u.discord_name,
            permissions: u.permissions,
        })
        .into_response(),
        Err((c, e)) => (c, e).into_response(),
    }
}

async fn list_users(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    Json(storage::public_users()).into_response()
}

async fn add_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<NewUserRequest>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::add_user(input) {
        Ok(u) => (StatusCode::CREATED, Json(u)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn update_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(username): Path<String>,
    Json(input): Json<UpdateUserRequest>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::update_user(&username, input) {
        Ok(Some(u)) => Json(u).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn delete_user(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::delete_user(&username) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn get_config(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    Json(storage::load_config()).into_response()
}

async fn save_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(config): Json<BotConfig>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::save_config(&config) {
        Ok(_) => (StatusCode::OK, Json(config)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn bot_status(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    if current_user(&state, &headers).await.is_err() {
        return (StatusCode::UNAUTHORIZED, "login required").into_response();
    }
    Json(state.bot.status().await).into_response()
}

async fn connect_bot(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match state.bot.connect_all(storage::load_config()) {
        Ok(_) => (StatusCode::OK, "connecting").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn disconnect_bot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match state.bot.disconnect_all() {
        Ok(_) => (StatusCode::OK, "disconnecting").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}


async fn halt_bot_action(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let _user = current_user(&state, &headers).await?;
    state
        .bot
        .halt_current_action()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({
        "mesadmin": "Halt requested. Current walking/action loops will stop at the next safe check."
    })))
}

async fn hard_stop_bot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    let _ = state.bot.disconnect_all();
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(350));
        if let Err(e) = schedule_restart() {
            eprintln!("Hard stop restart scheduling failed: {e}");
        }
        eprintln!("Hard stop requested from GUI; exiting process so Azalea runtime cannot reconnect. A restart has been scheduled.");
        std::process::exit(0);
    });
    (
        StatusCode::OK,
        "hard stopping: the GUI process will exit, kill all bot runtimes, then restart",
    )
        .into_response()
}

fn schedule_restart() -> std::io::Result<()> {
    let exe = env::current_exe()?;
    let cwd = env::current_dir()?;
    Command::new(exe)
        .current_dir(cwd)
        .env("PEARL_STASIS_RESTART_DELAY_MS", "1500")
        .spawn()?;
    Ok(())
}

async fn connect_bot_account(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    let config = storage::load_config();
    let wanted = name.trim().to_lowercase();
    let Some(account) = config
        .normalized_accounts()
        .into_iter()
        .find(|a| a.name.trim().eq_ignore_ascii_case(&wanted))
    else {
        return (
            StatusCode::NOT_FOUND,
            format!("bot account '{wanted}' not found"),
        )
            .into_response();
    };
    match state.bot.connect_one(account) {
        Ok(_) => (StatusCode::OK, "connecting bot account").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn disconnect_bot_account(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match state.bot.disconnect_one(name) {
        Ok(_) => (StatusCode::OK, "disconnecting bot account").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn hard_stop_bot_account(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match state.bot.hard_stop_one(name) {
        Ok(_) => (
            StatusCode::OK,
            "hard-stopped bot account; reconnect disabled until Connect is pressed again",
        )
            .into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn bot_chat(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match state.bot.chat(body) {
        Ok(_) => (StatusCode::OK, "sent").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn list_pearls(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    match current_user(&state, &headers).await {
        Ok(u) => Json(storage::visible_pearls(&u.username, &u.role)).into_response(),
        Err((c, e)) => (c, e).into_response(),
    }
}

async fn add_pearl(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<NewPearlEntry>,
) -> impl IntoResponse {
    let user = match current_user(&state, &headers).await {
        Ok(u) => u,
        Err((c, e)) => return (c, e).into_response(),
    };
    match storage::add_pearl(input, user.username, &user.role) {
        Ok(p) => (StatusCode::CREATED, Json(p)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn update_pearl(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<NewPearlEntry>,
) -> impl IntoResponse {
    let user = match current_user(&state, &headers).await {
        Ok(u) => u,
        Err((c, e)) => return (c, e).into_response(),
    };
    match storage::update_pearl(id, input, &user.username, &user.role) {
        Ok(Some(p)) => Json(p).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn delete_pearl(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let user = match current_user(&state, &headers).await {
        Ok(u) => u,
        Err((c, e)) => return (c, e).into_response(),
    };
    match storage::delete_pearl(id, &user.username, &user.role) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn pull_pearl(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let user = match current_user(&state, &headers).await {
        Ok(u) => u,
        Err((c, e)) => return (c, e).into_response(),
    };
    let Some(pearl) = storage::find_visible_pearl(id, &user.username, &user.role) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let pearl_for_pull = pearl.clone();
    let pull_result = state.bot.pull(pearl_for_pull);
    match pull_result {
        Ok(_) => (StatusCode::OK, "pulling").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn throw_stasis(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let user = match current_user(&state, &headers).await {
        Ok(u) => u,
        Err((c, e)) => return (c, e).into_response(),
    };
    let Some(pearl) = storage::find_visible_pearl(id, &user.username, &user.role) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match state.bot.throw_stasis(pearl) {
        Ok(_) => (StatusCode::OK, "throwing stasis item").into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn list_waypoints(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err((c, e)) = current_user(&state, &headers).await {
        return (c, e).into_response();
    }
    Json(storage::load_waypoints()).into_response()
}

async fn add_waypoint(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<NewWaypointEntry>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::add_waypoint(input) {
        Ok(w) => (StatusCode::CREATED, Json(w)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn update_waypoint(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<NewWaypointEntry>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::update_waypoint(id, input) {
        Ok(Some(w)) => Json(w).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn delete_waypoint(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::delete_waypoint(id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn walk_to_waypoint(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    let Some(waypoint) = storage::find_waypoint(id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let response = crate::models::WalkToWaypointResponse {
        mesadmin: format!(
            "Started walking '{}' in the background. Watch bot status/log output for progress.",
            waypoint.label
        ),
        bot_name: waypoint.bot_name.clone(),
        label: waypoint.label.clone(),
        x: waypoint.x,
        y: waypoint.y,
        z: waypoint.z,
    };
    let bot = state.bot.clone();
    tokio::spawn(async move {
        if let Err(e) = bot.walk_to_waypoint(waypoint).await {
            eprintln!("background waypoint walk failed: {e}");
        }
    });
    Json(response).into_response()
}

async fn casino_balance(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user = match current_user(&state, &headers).await {
        Ok(u) => u,
        Err((c, e)) => return (c, e).into_response(),
    };
    match casino::balance(&user.username) {
        Ok(balance) => Json(balance).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn casino_roulette(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<RouletteRequest>,
) -> impl IntoResponse {
    let user = match current_user(&state, &headers).await {
        Ok(u) => u,
        Err((c, e)) => return (c, e).into_response(),
    };
    match casino::play_roulette(&user.username, input.bet, &input.choice) {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn casino_slots(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<SlotsRequest>,
) -> impl IntoResponse {
    let user = match current_user(&state, &headers).await {
        Ok(u) => u,
        Err((c, e)) => return (c, e).into_response(),
    };
    match casino::play_slots(&user.username, input.bet) {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn blackjack_start(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<BlackjackStartRequest>,
) -> impl IntoResponse {
    let user = match current_user(&state, &headers).await {
        Ok(u) => u,
        Err((c, e)) => return (c, e).into_response(),
    };
    match casino::blackjack_start(&user.username, input.bet) {
        Ok((game, response)) => {
            if response.finished {
                state
                    .blackjack_sessions
                    .write()
                    .await
                    .remove(&user.username);
            } else {
                state
                    .blackjack_sessions
                    .write()
                    .await
                    .insert(user.username, game);
            }
            Json(response).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn blackjack_hit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user = match current_user(&state, &headers).await {
        Ok(u) => u,
        Err((c, e)) => return (c, e).into_response(),
    };
    let mut sessions = state.blackjack_sessions.write().await;
    let Some(game) = sessions.get_mut(&user.username) else {
        return (StatusCode::BAD_REQUEST, "no active blackjack hand").into_response();
    };
    match casino::blackjack_hit(&user.username, game) {
        Ok(response) => {
            if response.finished {
                sessions.remove(&user.username);
            }
            Json(response).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn blackjack_stand(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user = match current_user(&state, &headers).await {
        Ok(u) => u,
        Err((c, e)) => return (c, e).into_response(),
    };
    let mut sessions = state.blackjack_sessions.write().await;
    let Some(mut game) = sessions.remove(&user.username) else {
        return (StatusCode::BAD_REQUEST, "no active blackjack hand").into_response();
    };
    match casino::blackjack_stand(&user.username, &mut game) {
        Ok(response) => Json(response).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn bank_balances(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = feature_user(&state, &headers, "bank").await {
        return e.into_response();
    }
    match casino::all_balances() {
        Ok(rows) => Json(rows).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn set_bank_balance(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<UpdateBankBalanceRequest>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match casino::set_balance(&input.username, input.credits) {
        Ok(row) => Json(row).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn vip_table(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(e) = current_user(&state, &headers).await {
        return e.into_response();
    }
    match casino::all_balances() {
        Ok(rows) => {
            let table: Vec<VipTableEntry> = rows
                .into_iter()
                .take(25)
                .enumerate()
                .map(|(i, balance)| VipTableEntry {
                    rank: i + 1,
                    username: balance.username,
                    credits: balance.credits,
                })
                .collect();
            Json(table).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn list_shop_items(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = feature_user(&state, &headers, "shop").await {
        return e.into_response();
    }
    Json(storage::load_shop_items()).into_response()
}

async fn add_shop_item(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<NewShopItemEntry>,
) -> impl IntoResponse {
    if let Err(e) = owner_user(&state, &headers).await {
        return e.into_response();
    }
    match storage::add_shop_item(input) {
        Ok(item) => Json(item).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn update_shop_item(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<NewShopItemEntry>,
) -> impl IntoResponse {
    if let Err(e) = owner_user(&state, &headers).await {
        return e.into_response();
    }
    match storage::update_shop_item(id, input) {
        Ok(Some(item)) => Json(item).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "shop item not found").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn delete_shop_item(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(e) = owner_user(&state, &headers).await {
        return e.into_response();
    }
    match storage::delete_shop_item(id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "shop item not found").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn list_bounties(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = feature_user(&state, &headers, "bounty").await {
        return e.into_response();
    }
    Json(storage::load_bounties()).into_response()
}

async fn add_bounty(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<NewBountyEntry>,
) -> impl IntoResponse {
    let user = match owner_user(&state, &headers).await {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    match storage::add_bounty(input, user.username) {
        Ok(item) => Json(item).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn update_bounty(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<NewBountyEntry>,
) -> impl IntoResponse {
    let user = match owner_user(&state, &headers).await {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    match storage::update_bounty(id, input, user.username) {
        Ok(Some(item)) => Json(item).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "bounty not found").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn delete_bounty(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(e) = owner_user(&state, &headers).await {
        return e.into_response();
    }
    match storage::delete_bounty(id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "bounty not found").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn process_chest_ledger(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<ChestLedgerConfig>,
) -> impl IntoResponse {
    if let Err((c, e)) = feature_user(&state, &headers, "bank").await {
        return (c, e).into_response();
    }
    let label = input.label.clone();
    let bot = state.bot.clone();
    tokio::spawn(async move {
        match bot.process_chest_ledger(input).await {
            Ok(result) => println!("{}", result.mesadmin),
            Err(e) => eprintln!("background chest ledger failed: {e}"),
        }
    });
    Json(crate::models::ChestLedgerResponse {
        mesadmin: format!(
            "Started bank ledger '{}' in the background. Watch bot status/log output for processed/skipped details.",
            label
        ),
        processed: vec![],
        skipped: vec![],
    })
    .into_response()
}

async fn list_ledger_chests(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err((c, e)) = feature_any_user(&state, &headers, &["bank", "book_writer"]).await {
        return (c, e).into_response();
    }
    Json(storage::load_ledger_chests()).into_response()
}

async fn add_ledger_chest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<NewLedgerChestEntry>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::add_ledger_chest(input) {
        Ok(entry) => Json(entry).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn update_ledger_chest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<NewLedgerChestEntry>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::update_ledger_chest(id, input) {
        Ok(Some(entry)) => Json(entry).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "ledger chest not found").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn delete_ledger_chest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::delete_ledger_chest(id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "ledger chest not found").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn process_ledger_chest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err((c, e)) = feature_user(&state, &headers, "bank").await {
        return (c, e).into_response();
    }
    let Some(entry) = storage::find_ledger_chest(id) else {
        return (StatusCode::NOT_FOUND, "ledger chest not found").into_response();
    };
    if !entry.purpose.eq_ignore_ascii_case("banking") {
        return (
            StatusCode::BAD_REQUEST,
            "this saved chest is a writer chest; choose a banking chest to run the ledger",
        )
            .into_response();
    }
    let label = entry.label.clone();
    let config = entry.into();
    let bot = state.bot.clone();
    tokio::spawn(async move {
        match bot.process_chest_ledger(config).await {
            Ok(result) => println!("{}", result.mesadmin),
            Err(e) => eprintln!("background saved ledger failed: {e}"),
        }
    });
    Json(crate::models::ChestLedgerResponse {
        mesadmin: format!(
            "Started bank ledger '{}' in the background. Watch bot status/log output for processed/skipped details.",
            label
        ),
        processed: vec![],
        skipped: vec![],
    })
    .into_response()
}

async fn walk_ledger_chest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err((c, e)) = feature_any_user(&state, &headers, &["bank", "book_writer"]).await {
        return (c, e).into_response();
    }
    let Some(entry) = storage::find_ledger_chest(id) else {
        return (StatusCode::NOT_FOUND, "ledger chest not found").into_response();
    };
    let waypoint = WaypointEntry {
        id: entry.id,
        label: format!("{} chest", entry.label),
        category: "Ledger Chests".to_string(),
        bot_name: entry.bot_name,
        x: entry.chest_x,
        y: entry.chest_y,
        z: entry.chest_z,
        notes: format!("{} chest waypoint", entry.purpose),
        created_at: entry.created_at,
    };
    let response = crate::models::WalkToWaypointResponse {
        mesadmin: format!(
            "Started walking to '{}' in the background. Watch bot status/log output for progress.",
            waypoint.label
        ),
        bot_name: waypoint.bot_name.clone(),
        label: waypoint.label.clone(),
        x: waypoint.x,
        y: waypoint.y,
        z: waypoint.z,
    };
    let bot = state.bot.clone();
    tokio::spawn(async move {
        if let Err(e) = bot.walk_to_waypoint(waypoint).await {
            eprintln!("background ledger chest walk failed: {e}");
        }
    });
    Json(response).into_response()
}

async fn write_sign_place_book(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<BookWriterRequest>,
) -> impl IntoResponse {
    if let Err((c, e)) = feature_user(&state, &headers, "book_writer").await {
        return (c, e).into_response();
    }
    match state.bot.write_sign_place_book(input).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn list_butler_chests(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    Json(storage::load_butler_chests()).into_response()
}

async fn add_butler_chest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<NewButlerChestEntry>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::add_butler_chest(input) {
        Ok(entry) => Json(entry).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn update_butler_chest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<NewButlerChestEntry>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::update_butler_chest(id, input) {
        Ok(Some(entry)) => Json(entry).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "butler source chest not found").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn delete_butler_chest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::delete_butler_chest(id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "butler source chest not found").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn walk_butler_chest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    let Some(entry) = storage::find_butler_chest(id) else {
        return (StatusCode::NOT_FOUND, "butler source chest not found").into_response();
    };
    let waypoint = WaypointEntry {
        id: entry.id,
        label: format!("{} source chest", entry.label),
        category: "Bank Butler".to_string(),
        bot_name: entry.bot_name,
        x: entry.chest_x,
        y: entry.chest_y,
        z: entry.chest_z,
        notes: "bank butler source chest waypoint".into(),
        created_at: entry.created_at,
    };
    let response = crate::models::WalkToWaypointResponse {
        mesadmin: format!(
            "Started walking to '{}' in the background. Watch bot status/log output for progress.",
            waypoint.label
        ),
        bot_name: waypoint.bot_name.clone(),
        label: waypoint.label.clone(),
        x: waypoint.x,
        y: waypoint.y,
        z: waypoint.z,
    };
    let bot = state.bot.clone();
    tokio::spawn(async move {
        if let Err(e) = bot.walk_to_waypoint(waypoint).await {
            eprintln!("background butler source walk failed: {e}");
        }
    });
    Json(response).into_response()
}

async fn list_butler_waypoints(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    Json(storage::load_butler_waypoints()).into_response()
}

async fn add_butler_waypoint(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<NewButlerWaypointEntry>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::add_butler_waypoint(input) {
        Ok(entry) => Json(entry).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn update_butler_waypoint(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<NewButlerWaypointEntry>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::update_butler_waypoint(id, input) {
        Ok(Some(entry)) => Json(entry).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "butler waypoint not found").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn delete_butler_waypoint(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match storage::delete_butler_waypoint(id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "butler waypoint not found").into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

async fn walk_butler_waypoint(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    let Some(entry) = storage::find_butler_waypoint(id) else {
        return (
            StatusCode::NOT_FOUND,
            "butler destination waypoint not found",
        )
            .into_response();
    };
    let waypoint = WaypointEntry {
        id: entry.id,
        label: format!("{} destination chest", entry.label),
        category: "Bank Butler".to_string(),
        bot_name: entry.bot_name,
        x: entry.chest_x,
        y: entry.chest_y,
        z: entry.chest_z,
        notes: "bank butler destination chest waypoint".into(),
        created_at: entry.created_at,
    };
    let response = crate::models::WalkToWaypointResponse {
        mesadmin: format!(
            "Started walking to '{}' in the background. Watch bot status/log output for progress.",
            waypoint.label
        ),
        bot_name: waypoint.bot_name.clone(),
        label: waypoint.label.clone(),
        x: waypoint.x,
        y: waypoint.y,
        z: waypoint.z,
    };
    let bot = state.bot.clone();
    tokio::spawn(async move {
        if let Err(e) = bot.walk_to_waypoint(waypoint).await {
            eprintln!("background butler destination walk failed: {e}");
        }
    });
    Json(response).into_response()
}

async fn butler_transfer(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<ButlerTransferRequest>,
) -> impl IntoResponse {
    if let Err((c, e)) = owner_user(&state, &headers).await {
        return (c, e).into_response();
    }
    let shulker_name = input.shulker_name.clone();
    let bot = state.bot.clone();
    tokio::spawn(async move {
        match bot.butler_transfer(input).await {
            Ok(result) => println!("{}", result.mesadmin),
            Err(e) => eprintln!("background bank butler transfer failed: {e}"),
        }
    });
    Json(crate::models::ButlerTransferResponse {
        mesadmin: format!(
            "Started Bank Butler transfer for '{shulker_name}' in the background. Watch bot status/log output for progress."
        ),
        bot_name: String::new(),
        shulker_name,
        source_label: String::new(),
        destination_label: String::new(),
    })
    .into_response()
}

async fn viewport_snapshot(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<ViewportSnapshotRequest>,
) -> impl IntoResponse {
    if let Err((c, e)) = current_user(&state, &headers).await {
        return (c, e).into_response();
    }
    match state.bot.viewport_snapshot(input).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}


fn allowed_public_collection(name: &str) -> bool {
    matches!(name,
        "chronicles"|"library"|"decrees"|"historical_events"|"timeline"|"archive_backups"|
        "players"|"factions"|"intelligence_reports"|"sightings"|"watchlist"|"contacts"|
        "regions"|"roads"|"portals"|"ice_highways"|"routes"|"regional_ownership"|
        "admins"|"former_admins"|"former_archadmins"|"founders"|"benefactors"|"lineage"|"mentors"|
        "relics"|"artifacts"|"banners"|"codices"|"wonders"|
        "proposals"|"votes"|"decisions"|"census"|"citizens"|"ranks"|"promotions"|
        "mail"|"notifications"|"news"|"discoveries"|"explorer_rankings"|"research"|"scholar_contributions"|
        "id_cards"|"certificates"|"archive_scans"|"pearl_monitor"|"assets"|"emergency_log"|
        "departments"|"budgets"|"financial_reports"|"faqs"|"login_greeters"
    )
}

fn public_collection_label(name: &str) -> &'static str {
    match name {
        "chronicles" => "Chronicles", "library" => "Server Library", "decrees" => "Server Decrees",
        "historical_events" => "Historical Events", "timeline" => "Timeline", "archive_backups" => "Archive Backups",
        "players" => "Player Registry", "factions" => "Faction Registry", "intelligence_reports" => "Intelligence Reports",
        "sightings" => "Sightings", "watchlist" => "Watchlist", "contacts" => "Contact Records",
        "regions" => "Regions", "roads" => "Road Registry", "portals" => "Portal Registry", "ice_highways" => "Ice Highway Registry",
        "routes" => "Route Documentation", "regional_ownership" => "Regional Ownership", "admins" => "Hall of Admins",
        "former_admins" => "Former Admins", "former_archadmins" => "Former Senior Admins", "founders" => "Founders",
        "benefactors" => "Benefactors", "lineage" => "Lineage Tree", "mentors" => "Mentor Relationships",
        "relics" => "Relics", "artifacts" => "Artifacts", "banners" => "Banners", "codices" => "Codices",
        "wonders" => "World Wonders", "proposals" => "Council Proposals", "votes" => "Votes", "decisions" => "Decision Archive",
        "census" => "Census", "citizens" => "Citizen Registry", "ranks" => "Rank Management", "promotions" => "Promotions",
        "mail" => "Server Postal Service", "notifications" => "Notifications", "news" => "News Network",
        "discoveries" => "Discoveries", "explorer_rankings" => "Explorer Rankings", "research" => "Research Entries",
        "scholar_contributions" => "Scholar Contributions", "id_cards" => "Server ID Cards", "certificates" => "Server Certificates",
        "archive_scans" => "Archive Scanner", "pearl_monitor" => "Pearl Network Monitor", "assets" => "Server Asset Registry",
        "emergency_log" => "Emergency Log", "departments" => "Department Accounts", "budgets" => "Budget Allocation",
        "financial_reports" => "Financial Reports", "faqs" => "FAQ Database", "login_greeters" => "Login Greeter", _ => "Server Records"
    }
}

fn require_public_read(user: &AppUser, _collection: &str) -> bool {
    user.role == UserRole::Owner
}

fn require_public_write(user: &AppUser, _collection: &str) -> bool {
    user.role == UserRole::Owner
}

async fn faq_public_list() -> impl IntoResponse {
    let rows = storage::public_load("faqs").unwrap_or_default();
    let visible: Vec<Value> = rows.into_iter().filter(|r| {
        r.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true)
    }).collect();
    Json(visible).into_response()
}

async fn greeter_public_list() -> impl IntoResponse {
    let rows = storage::public_load("login_greeters").unwrap_or_default();
    let visible: Vec<Value> = rows.into_iter().filter(|r| {
        r.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true)
    }).collect();
    Json(visible).into_response()
}

async fn public_collections(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    let Ok(user) = current_user(&state, &headers).await else { return StatusCode::UNAUTHORIZED.into_response(); };
    let cols = storage::public_collections()
        .into_iter()
        .filter(|c| require_public_read(&user, c))
        .map(|c| serde_json::json!({"id": c, "label": public_collection_label(c)}))
        .collect::<Vec<_>>();
    Json(cols).into_response()
}

async fn public_dashboard(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    let Ok(user) = current_user(&state, &headers).await else { return StatusCode::UNAUTHORIZED.into_response(); };
    let mut out = serde_json::Map::new();
    for c in storage::public_collections() {
        if require_public_read(&user, c) {
            let count = storage::public_load(c).map(|v| v.len()).unwrap_or(0);
            out.insert(c.to_string(), serde_json::json!({"label": public_collection_label(c), "count": count}));
        }
    }
    Json(Value::Object(out)).into_response()
}

async fn public_list(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(collection): Path<String>) -> impl IntoResponse {
    if !allowed_public_collection(&collection) { return (StatusCode::NOT_FOUND, "unknown server collection").into_response(); }
    let user = match current_user(&state, &headers).await { Ok(u) => u, Err((c,e)) => return (c,e).into_response() };
    if !require_public_read(&user, &collection) { return (StatusCode::FORBIDDEN, "server records permission required").into_response(); }
    match storage::public_load(&collection) { Ok(rows) => Json(rows).into_response(), Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response() }
}

async fn public_add(State(state): State<Arc<AppState>>, headers: HeaderMap, Path(collection): Path<String>, Json(mut row): Json<Value>) -> impl IntoResponse {
    if !allowed_public_collection(&collection) { return (StatusCode::NOT_FOUND, "unknown server collection").into_response(); }
    let user = match current_user(&state, &headers).await { Ok(u) => u, Err((c,e)) => return (c,e).into_response() };
    if !require_public_write(&user, &collection) { return (StatusCode::FORBIDDEN, "server admin permission required").into_response(); }
    if !row.is_object() { row = serde_json::json!({"value": row}); }
    match storage::public_add(&collection, row, &user.username) { Ok(row) => (StatusCode::CREATED, Json(row)).into_response(), Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response() }
}

async fn public_update(State(state): State<Arc<AppState>>, headers: HeaderMap, Path((collection, id)): Path<(String, String)>, Json(mut row): Json<Value>) -> impl IntoResponse {
    if !allowed_public_collection(&collection) { return (StatusCode::NOT_FOUND, "unknown server collection").into_response(); }
    let user = match current_user(&state, &headers).await { Ok(u) => u, Err((c,e)) => return (c,e).into_response() };
    if !require_public_write(&user, &collection) { return (StatusCode::FORBIDDEN, "server admin permission required").into_response(); }
    if !row.is_object() { row = serde_json::json!({"value": row}); }
    match storage::public_update(&collection, &id, row, &user.username) { Ok(Some(row)) => Json(row).into_response(), Ok(None) => (StatusCode::NOT_FOUND, "record not found").into_response(), Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response() }
}

async fn public_delete(State(state): State<Arc<AppState>>, headers: HeaderMap, Path((collection, id)): Path<(String, String)>) -> impl IntoResponse {
    if !allowed_public_collection(&collection) { return (StatusCode::NOT_FOUND, "unknown server collection").into_response(); }
    let user = match current_user(&state, &headers).await { Ok(u) => u, Err((c,e)) => return (c,e).into_response() };
    if !require_public_write(&user, &collection) { return (StatusCode::FORBIDDEN, "server admin permission required").into_response(); }
    match storage::public_delete(&collection, &id, &user.username) { Ok(true) => StatusCode::NO_CONTENT.into_response(), Ok(false) => (StatusCode::NOT_FOUND, "record not found").into_response(), Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response() }
}

async fn public_search(State(state): State<Arc<AppState>>, headers: HeaderMap, Json(req): Json<Value>) -> impl IntoResponse {
    let user = match current_user(&state, &headers).await { Ok(u) => u, Err((c,e)) => return (c,e).into_response() };
    let query = req.get("query").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
    let mut hits = Vec::new();
    if query.trim().is_empty() { return Json(hits).into_response(); }
    for c in storage::public_collections() {
        if !require_public_read(&user, c) { continue; }
        if let Ok(rows) = storage::public_load(c) {
            for row in rows {
                let text = serde_json::to_string(&row).unwrap_or_default().to_lowercase();
                if text.contains(&query) {
                    hits.push(serde_json::json!({"collection": c, "label": public_collection_label(c), "record": row}));
                }
            }
        }
    }
    Json(hits).into_response()
}
