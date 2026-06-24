use crate::{
    casino,
    models::{
        BookWriterRequest, BookWriterResponse, BotAccountConfig, BotAccountStatus, BotConfig,
        BotStatus, ButlerChestEntry, ButlerTransferRequest, ButlerTransferResponse,
        ButlerWaypointEntry, ChestLedgerConfig, ChestLedgerEntry, ChestLedgerResponse, PearlEntry,
        ViewportBlock, ViewportSnapshot, ViewportSnapshotRequest, WalkToWaypointResponse,
        WaypointEntry, WorldClockStatus,
    },
    storage,
};
use anyhow::{anyhow, Result};
use azalea::inventory::operations::{SwapClick, ThrowClick};
use azalea::{
    account::Account, prelude::PathfinderClientExt, BlockPos, Client, Event, ClientMovementState,
    SprintDirection, WalkDirection,
};
use azalea_block::{BlockState, BlockTrait};
use azalea_client::local_player::Hunger;
use azalea_entity::Position;
use azalea_inventory::{
    components::{Consumable, CustomName, Food, WritableBookContent, WrittenBookContent},
    ItemStack, Menu,
};
use azalea_protocol::packets::game::{s_edit_book::ServerboundEditBook, ClientboundGamePacket};
use azalea_registry::builtin::ItemKind;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{mpsc, RwLock},
    time::timeout,
};

#[derive(Clone)]
pub struct BotController {
    tx: mpsc::UnboundedSender<BotCommand>,
    status: Arc<RwLock<BotStatus>>,
    halt_flag: Arc<AtomicBool>,
}

enum BotCommand {
    SnapshotStatus(tokio::sync::oneshot::Sender<BotStatus>),
    ConnectAll(BotConfig),
    ConnectOne(BotAccountConfig),
    DisconnectAll,
    DisconnectOne(String),
    HardStopOne(String),
    Pull(PearlEntry, Option<WaypointEntry>),
    ThrowStasis(PearlEntry),
    WalkToWaypoint(
        WaypointEntry,
        tokio::sync::oneshot::Sender<std::result::Result<WalkToWaypointResponse, String>>,
    ),
    ProcessChestLedger(
        ChestLedgerConfig,
        tokio::sync::oneshot::Sender<std::result::Result<ChestLedgerResponse, String>>,
    ),
    WriteSignPlaceBook(
        BookWriterRequest,
        tokio::sync::oneshot::Sender<std::result::Result<BookWriterResponse, String>>,
    ),
    ButlerTransfer(
        ButlerTransferRequest,
        tokio::sync::oneshot::Sender<std::result::Result<ButlerTransferResponse, String>>,
    ),
    ViewportSnapshot(
        ViewportSnapshotRequest,
        tokio::sync::oneshot::Sender<std::result::Result<ViewportSnapshot, String>>,
    ),
    Chat(String),
}

struct ConnectedBot {
    client: Client,
    username: String,
    mesadmin: String,
    event_task: tokio::task::JoinHandle<()>,
    world_time: Arc<RwLock<Option<WorldTimeSnapshot>>>,
    open_book_events: Arc<RwLock<u64>>,
}

struct BotRuntime {
    clients: HashMap<String, ConnectedBot>,
    manually_stopped: HashSet<String>,
    status: Arc<RwLock<BotStatus>>,
    command_tx: mpsc::UnboundedSender<BotCommand>,
    halt_flag: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
struct WorldTimeSnapshot {
    day_time: u64,
    time_of_day: u64,
    time_label: String,
    received_at: Instant,
}

impl BotController {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let status = Arc::new(RwLock::new(BotStatus {
            connected: false,
            username: None,
            mesadmin: "Not connected".into(),
            login_help: None,
            bots: vec![],
            world_time: None,
        }));
        let halt_flag = Arc::new(AtomicBool::new(false));
        let runtime = BotRuntime {
            clients: HashMap::new(),
            manually_stopped: HashSet::new(),
            status: status.clone(),
            command_tx: tx.clone(),
            halt_flag: halt_flag.clone(),
        };
        std::thread::Builder::new()
            .name("azalea-bot-runtime".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("failed to create bot runtime");
                    let local = tokio::task::LocalSet::new();
                    local.block_on(&rt, runtime.run(rx));
                }));
                if result.is_err() {
                    eprintln!(
                        "azalea-bot-runtime thread panicked. The GUI can stay open, but reconnect the bot or restart the EXE before sending more bot commands."
                    );
                }
            })
            .expect("failed to spawn bot runtime thread");
        Self { tx, status, halt_flag }
    }

    pub async fn status(&self) -> BotStatus {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self.tx.send(BotCommand::SnapshotStatus(reply_tx)).is_ok() {
            if let Ok(Ok(status)) =
                tokio::time::timeout(std::time::Duration::from_millis(750), reply_rx).await
            {
                return status;
            }
        }
        self.status.read().await.clone()
    }
    pub fn connect_all(&self, config: BotConfig) -> Result<()> {
        self.tx
            .send(BotCommand::ConnectAll(config))
            .map_err(|e| anyhow!(e.to_string()))
    }
    pub fn connect_one(&self, account: BotAccountConfig) -> Result<()> {
        self.tx
            .send(BotCommand::ConnectOne(account))
            .map_err(|e| anyhow!(e.to_string()))
    }
    pub fn disconnect_all(&self) -> Result<()> {
        self.tx
            .send(BotCommand::DisconnectAll)
            .map_err(|e| anyhow!(e.to_string()))
    }
    pub fn disconnect_one(&self, name: String) -> Result<()> {
        self.tx
            .send(BotCommand::DisconnectOne(name))
            .map_err(|e| anyhow!(e.to_string()))
    }
    pub fn hard_stop_one(&self, name: String) -> Result<()> {
        self.tx
            .send(BotCommand::HardStopOne(name))
            .map_err(|e| anyhow!(e.to_string()))
    }
    pub fn pull(&self, pearl: PearlEntry) -> Result<()> {
        self.halt_flag.store(false, Ordering::SeqCst);
        let return_waypoint = auto_return_waypoint_for_pearl(&pearl);
        self.tx
            .send(BotCommand::Pull(pearl, return_waypoint))
            .map_err(|e| anyhow!(e.to_string()))
    }
    pub fn throw_stasis(&self, pearl: PearlEntry) -> Result<()> {
        self.halt_flag.store(false, Ordering::SeqCst);
        self.tx
            .send(BotCommand::ThrowStasis(pearl))
            .map_err(|e| anyhow!(e.to_string()))
    }
    pub async fn walk_to_waypoint(
        &self,
        waypoint: WaypointEntry,
    ) -> Result<WalkToWaypointResponse> {
        self.halt_flag.store(false, Ordering::SeqCst);
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(BotCommand::WalkToWaypoint(waypoint, reply_tx))
            .map_err(|e| anyhow!(e.to_string()))?;
        match tokio::time::timeout(std::time::Duration::from_secs(1800), reply_rx).await {
            Ok(Ok(result)) => result.map_err(|e| anyhow!(e)),
            Ok(Err(e)) => Err(anyhow!("bot runtime stopped during waypoint walk: {e}")),
            Err(_) => Err(anyhow!(
                "waypoint walk timed out after 30 minutes waiting for the bot runtime; check the bot console for the latest walking status"
            )),
        }
    }
    pub async fn process_chest_ledger(
        &self,
        config: ChestLedgerConfig,
    ) -> Result<ChestLedgerResponse> {
        self.halt_flag.store(false, Ordering::SeqCst);
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(BotCommand::ProcessChestLedger(config, reply_tx))
            .map_err(|e| anyhow!(e.to_string()))?;
        reply_rx
            .await
            .map_err(|e| anyhow!(e.to_string()))?
            .map_err(|e| anyhow!(e))
    }
    pub async fn write_sign_place_book(
        &self,
        request: BookWriterRequest,
    ) -> Result<BookWriterResponse> {
        self.halt_flag.store(false, Ordering::SeqCst);
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(BotCommand::WriteSignPlaceBook(request, reply_tx))
            .map_err(|e| anyhow!(e.to_string()))?;
        reply_rx
            .await
            .map_err(|e| anyhow!(e.to_string()))?
            .map_err(|e| anyhow!(e))
    }
    pub async fn butler_transfer(
        &self,
        request: ButlerTransferRequest,
    ) -> Result<ButlerTransferResponse> {
        self.halt_flag.store(false, Ordering::SeqCst);
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(BotCommand::ButlerTransfer(request, reply_tx))
            .map_err(|e| anyhow!(e.to_string()))?;
        match tokio::time::timeout(std::time::Duration::from_secs(1800), reply_rx).await {
            Ok(Ok(result)) => result.map_err(|e| anyhow!(e)),
            Ok(Err(e)) => Err(anyhow!("bot runtime stopped during bank butler transfer: {e}")),
            Err(_) => Err(anyhow!(
                "bank butler transfer timed out after 30 minutes; check the bot console for walking status"
            )),
        }
    }
    pub async fn viewport_snapshot(
        &self,
        request: ViewportSnapshotRequest,
    ) -> Result<ViewportSnapshot> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(BotCommand::ViewportSnapshot(request, reply_tx))
            .map_err(|e| anyhow!(e.to_string()))?;
        timeout(Duration::from_secs(4), reply_rx)
            .await
            .map_err(|_| anyhow!("viewport snapshot timed out because the bot runtime is busy; try again after the current walk/action finishes"))?
            .map_err(|e| anyhow!(e.to_string()))?
            .map_err(|e| anyhow!(e))
    }
    pub fn chat(&self, msg: String) -> Result<()> {
        self.tx
            .send(BotCommand::Chat(msg))
            .map_err(|e| anyhow!(e.to_string()))
    }

    pub fn halt_current_action(&self) -> Result<()> {
        self.halt_flag.store(true, Ordering::SeqCst);
        Ok(())
    }
}


impl BotRuntime {
    fn halt_if_requested(&self, label: &str) -> Result<()> {
        if self.halt_flag.load(Ordering::SeqCst) {
            for bot in self.clients.values() {
                bot.client.walk(WalkDirection::None);
                let _ = bot.client.set_jumping(false);
                bot.client.force_stop_pathfinding();
            }
            return Err(anyhow!("halt requested; stopped current bot action: {label}"));
        }
        Ok(())
    }

    async fn publish_status(&self, mesadmin: impl Into<String>, login_help: Option<String>) {
        let mut world_time = None;
        for bot in self.clients.values() {
            if let Some(snapshot) = bot.world_time.read().await.clone() {
                let snapshot = estimate_world_time(snapshot);
                world_time = Some(WorldClockStatus {
                    day_time: snapshot.day_time as i64,
                    time_of_day: snapshot.time_of_day as i64,
                    time_label: snapshot.time_label,
                    is_day: snapshot.time_of_day < 12_000,
                });
                break;
            }
        }
        let mut bots: Vec<BotAccountStatus> = self
            .clients
            .iter()
            .map(|(name, bot)| {
                let pos = bot.client.get_component::<Position>().map(|p| **p);
                BotAccountStatus {
                    name: name.clone(),
                    connected: true,
                    username: Some(bot.username.clone()),
                    mesadmin: bot.mesadmin.clone(),
                    x: pos.map(|p| p.x),
                    y: pos.map(|p| p.y),
                    z: pos.map(|p| p.z),
                }
            })
            .collect();
        bots.sort_by(|a, b| a.name.cmp(&b.name));
        let connected = !self.clients.is_empty();
        let username = if self.clients.len() == 1 {
            self.clients.values().next().map(|b| b.username.clone())
        } else {
            None
        };
        *self.status.write().await = BotStatus {
            connected,
            username,
            mesadmin: mesadmin.into(),
            login_help,
            bots,
            world_time,
        };
    }

    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<BotCommand>) {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                BotCommand::SnapshotStatus(reply) => {
                    let existing = self.status.read().await.clone();
                    self.publish_status(existing.mesadmin, existing.login_help)
                        .await;
                    let _ = reply.send(self.status.read().await.clone());
                }
                BotCommand::ConnectAll(config) => {
                    let accounts = config.normalized_accounts();
                    let enabled: Vec<_> = accounts.into_iter().filter(|a| a.enabled).collect();
                    if enabled.is_empty() {
                        self.publish_status("No enabled bot accounts in config", None)
                            .await;
                        continue;
                    }
                    for account in enabled {
                        if let Err(e) = self.connect_account(account.clone()).await {
                            eprintln!("connect '{}' failed: {e:?}", account.name);
                            self.publish_status(format!("Connect failed for {}: {e}", account.name), Some("For Microsoft auth: watch the console and complete the Microsoft device/browser login if shown.".into())).await;
                        }
                    }
                }
                BotCommand::ConnectOne(account) => {
                    if let Err(e) = self.connect_account(account.clone()).await {
                        eprintln!("connect '{}' failed: {e:?}", account.name);
                        self.publish_status(format!("Connect failed for {}: {e}", account.name), Some("For Microsoft auth: watch the console and complete the Microsoft device/browser login if shown.".into())).await;
                    }
                }
                BotCommand::DisconnectAll => {
                    let names: Vec<String> = self.clients.keys().cloned().collect();
                    for name in names {
                        self.manually_stopped.insert(name.clone());
                        if let Some(bot) = self.clients.remove(&name) {
                            bot.event_task.abort();
                            bot.client.disconnect();
                            println!("Manually disconnected bot {name}; reconnect disabled until Connect is pressed again");
                        }
                    }
                    self.publish_status(
                        "Disconnected all bots; reconnect disabled until Connect is pressed again",
                        None,
                    )
                    .await;
                }
                BotCommand::DisconnectOne(name) => {
                    let key = name.trim().to_lowercase();
                    self.manually_stopped.insert(key.clone());
                    if let Some(bot) = self.clients.remove(&key) {
                        bot.event_task.abort();
                        bot.client.disconnect();
                        println!("Manually disconnected bot {key}; reconnect disabled until Connect is pressed again");
                        self.publish_status(format!("Disconnected bot {key}; reconnect disabled until Connect is pressed again"), None).await;
                    } else {
                        self.publish_status(format!("Bot {key} was not connected; reconnect disabled until Connect is pressed again"), None).await;
                    }
                }

                BotCommand::HardStopOne(name) => {
                    let key = name.trim().to_lowercase();
                    self.manually_stopped.insert(key.clone());
                    if let Some(bot) = self.clients.remove(&key) {
                        bot.event_task.abort();
                        bot.client.disconnect();
                        // Drop the client handle after disconnect. This is the strongest per-bot stop
                        // possible without exiting the whole process. If Azalea still reconnects,
                        // use the global Hard Stop All Bots, which exits the process.
                        println!("Hard-stopped bot {key}; reconnect disabled until Connect is pressed again");
                        self.publish_status(format!("Hard-stopped bot {key}; reconnect disabled until Connect is pressed again"), None).await;
                    } else {
                        self.publish_status(format!("Bot {key} was not connected; reconnect disabled until Connect is pressed again"), None).await;
                    }
                }
                BotCommand::Pull(pearl, return_waypoint) => {
                    if let Err(e) = self.pull(pearl, return_waypoint).await {
                        self.publish_status(format!("Pull failed: {e}"), None).await;
                    }
                }
                BotCommand::ThrowStasis(pearl) => {
                    if let Err(e) = self.throw_stasis(pearl).await {
                        self.publish_status(format!("ThrowStasis failed: {e}"), None)
                            .await;
                    }
                }
                BotCommand::WalkToWaypoint(waypoint, reply) => {
                    let result = self.walk_to_waypoint(waypoint).await;
                    let _ = reply.send(result.map_err(|e| e.to_string()));
                }
                BotCommand::ProcessChestLedger(config, reply) => {
                    let result = self.process_chest_ledger(config).await;
                    let _ = reply.send(result.map_err(|e| e.to_string()));
                }
                BotCommand::WriteSignPlaceBook(request, reply) => {
                    let result = self.write_sign_place_book(request).await;
                    let _ = reply.send(result.map_err(|e| e.to_string()));
                }
                BotCommand::ButlerTransfer(request, reply) => {
                    let result = self.butler_transfer(request).await;
                    let _ = reply.send(result.map_err(|e| e.to_string()));
                }
                BotCommand::ViewportSnapshot(request, reply) => {
                    let result = self.viewport_snapshot(request).await;
                    let _ = reply.send(result.map_err(|e| e.to_string()));
                }
                BotCommand::Chat(msg) => {
                    if let Some((_name, bot)) = self.clients.iter().next() {
                        bot.client.chat(msg);
                    }
                }
            }
        }
    }

    async fn connect_account(&mut self, account_config: BotAccountConfig) -> Result<()> {
        let name = clean_bot_name(&account_config.name);
        self.manually_stopped.remove(&name);
        if let Some(old) = self.clients.remove(&name) {
            old.event_task.abort();
            old.client.disconnect();
        }
        self.publish_status(format!("Logging in / joining server with bot {name}..."), Some("If this is the first Microsoft login, complete the Microsoft device/browser login shown in the console.".into())).await;

        let account = if account_config.auth_mode.eq_ignore_ascii_case("offline") {
            Account::offline(&account_config.username_or_email)
        } else {
            Account::microsoft(&account_config.username_or_email).await?
        };

        let address = format!("{}:{}", account_config.host.trim(), account_config.port);
        let (client, mut events) = Client::join(account, address.clone()).await?;
        let username = account_config.username_or_email.clone();
        println!("Joined {address} as {username} [{name}]");
        self.publish_status(format!("Joined {address} as {username} [{name}]"), None)
            .await;

        let status = self.status.clone();
        let bot_name = name.clone();
        let event_client = client.clone();
        let event_command_tx = self.command_tx.clone();
        let world_time = Arc::new(RwLock::new(None));
        let event_world_time = world_time.clone();
        let open_book_events = Arc::new(RwLock::new(0_u64));
        let event_open_book_events = open_book_events.clone();
        let event_task = tokio::task::spawn_local(async move {
            let mut hunger_check = tokio::time::interval(Duration::from_secs(5));
            let mut faq_add_cooldowns: HashMap<String, Instant> = HashMap::new();
            let mut greeter_cooldowns: HashMap<String, Instant> = HashMap::new();
            loop {
                let event = tokio::select! {
                    _ = hunger_check.tick() => {
                        auto_eat_if_hungry(&event_client, &format!("idle {bot_name}"), false).await;
                        continue;
                    }
                    event = events.recv() => event,
                };
                let Some(event) = event else {
                    break;
                };
                match event {
                    Event::Disconnect(packet) => {
                        let mut s = status.write().await;
                        s.mesadmin = format!("Bot {bot_name} disconnected: {:?}", packet);
                        for b in &mut s.bots {
                            if b.name == bot_name {
                                b.connected = false;
                                b.mesadmin = format!("Disconnected: {:?}", packet);
                                b.x = None;
                                b.y = None;
                                b.z = None;
                            }
                        }
                        s.connected = s.bots.iter().any(|b| b.connected);
                        break;
                    }
                    Event::Chat(m) => {
                        let raw_line = m.mesadmin().to_ansi();
                        println!("chat[{bot_name}]: {raw_line}");
                        let line = normalize_incoming_chat_line(&raw_line);
                        if let Some((sender, text)) = parse_faq_add_command(&line) {
                            let config = storage::load_config();
                            if !config.faq_whisper_add_enabled {
                                send_configured_reply(&event_client, &config.faq_output_mode, &sender, "FAQ additions are disabled.");
                                continue;
                            }
                            if !minecraft_user_has_permission(&sender, "faq_whisper") {
                                send_configured_reply(&event_client, &config.faq_output_mode, &sender, "You are not allowed to edit Server FAQs.");
                                continue;
                            }
                            let cd = config.faq_whisper_cooldown_seconds;
                            if cd > 0 {
                                if let Some(last) = faq_add_cooldowns.get(&sender.to_lowercase()) {
                                    let remaining = cd.saturating_sub(last.elapsed().as_secs());
                                    if remaining > 0 {
                                        send_configured_reply(&event_client, &config.faq_output_mode, &sender, &format!("FAQ add cooldown: {remaining}s remaining."));
                                        continue;
                                    }
                                }
                            }
                            faq_add_cooldowns.insert(sender.to_lowercase(), Instant::now());
                            match storage::public_add("faqs", serde_json::json!({
                                "text": text,
                                "category": "Whisper Submitted",
                                "enabled": true,
                                "source": format!("Minecraft command from {sender}"),
                                "cooldown_seconds": cd
                            }), &sender) {
                                Ok(row) => {
                                    let number = faq_number_for_id(row.get("id").and_then(|v| v.as_str()).unwrap_or(""));
                                    let msg = match number { Some(n) => format!("Server FAQ #{n} added."), None => "Server FAQ added.".to_string() };
                                    send_configured_reply(&event_client, &config.faq_output_mode, &sender, &msg);
                                }
                                Err(e) => send_configured_reply(&event_client, &config.faq_output_mode, &sender, &format!("FAQ add failed: {e}")),
                            }
                            continue;
                        }
                        if let Some((sender, number, text)) = parse_faq_set_command(&line) {
                            let config = storage::load_config();
                            if !minecraft_user_has_permission(&sender, "faq_whisper") {
                                send_configured_reply(&event_client, &config.faq_output_mode, &sender, "You are not allowed to edit Server FAQs.");
                                continue;
                            }
                            let reply = set_faq_number(number, &text, &sender);
                            send_configured_reply(&event_client, &config.faq_output_mode, &sender, &reply);
                            continue;
                        }
                        if let Some((sender, number)) = parse_faq_delete_command(&line) {
                            let config = storage::load_config();
                            if !minecraft_user_has_permission(&sender, "faq_whisper") {
                                send_configured_reply(&event_client, &config.faq_output_mode, &sender, "You are not allowed to edit Server FAQs.");
                                continue;
                            }
                            let reply = delete_faq_number(number, &sender);
                            send_configured_reply(&event_client, &config.faq_output_mode, &sender, &reply);
                            continue;
                        }
                        if let Some((sender, query)) = parse_faq_query_command(&line) {
                            let config = storage::load_config();
                            let reply = answer_faq_query(&query);
                            send_configured_reply(&event_client, &config.faq_output_mode, &sender, &reply);
                            continue;
                        }
                        if let Some(sender) = parse_login_greeter_player(&line).or_else(|| parse_whisper_sender(&line)) {
                            let config = storage::load_config();
                            if config.login_greeter_enabled {
                                let cd = config.login_greeter_cooldown_seconds;
                                let key = sender.to_lowercase();
                                let allowed = if cd == 0 { true } else {
                                    greeter_cooldowns.get(&key).map(|t| t.elapsed().as_secs() >= cd).unwrap_or(true)
                                };
                                if allowed {
                                    if let Some(msg) = pick_login_greeter_mesadmin(&sender) {
                                        greeter_cooldowns.insert(key, Instant::now());
                                        send_configured_reply(&event_client, &config.greeter_output_mode, &sender, &msg);
                                    }
                                }
                            }
                        }
                        if let Some((sender, query)) = parse_whisper_pull_command(&line) {
                            match pearl_for_whisper_pull(&sender, query.as_deref()) {
                                Ok(pearl) => {
                                    let label = pearl.label.clone();
                                    let return_waypoint = auto_return_waypoint_for_pearl(&pearl);
                                    let _ = event_command_tx.send(BotCommand::Pull(pearl, return_waypoint));
                                    event_client.chat(format!(
                                        "/msg {sender} Pull queued for stasis pearl '{label}'."
                                    ));
                                }
                                Err(e) => {
                                    event_client.chat(format!("/msg {sender} Pull denied: {e}"));
                                }
                            }
                            continue;
                        }
                        if let Some((sender, reply)) = handle_casino_chat_command(&line) {
                            let config = storage::load_config();
                            if config.casino_public_chat {
                                event_client.chat(reply);
                            } else {
                                event_client.chat(format!("/msg {sender} {reply}"));
                            }
                        }
                    }
                    Event::Packet(packet) => {
                        if let ClientboundGamePacket::SetTime(time) = &*packet {
                            if let Some(state) = time.clock_updates.values().next() {
                                let time_of_day = state.total_ticks % 24_000;
                                let mut world_time = event_world_time.write().await;
                                *world_time = Some(WorldTimeSnapshot {
                                    day_time: state.total_ticks,
                                    time_of_day,
                                    time_label: minecraft_time_label(time_of_day),
                                    received_at: Instant::now(),
                                });
                            }
                        }
                        if let ClientboundGamePacket::OpenBook(open_book) = &*packet {
                            let mut open_book_events = event_open_book_events.write().await;
                            *open_book_events += 1;
                            println!(
                                "book-open[{bot_name}]: server opened book in hand {:?} (count {})",
                                open_book.hand, *open_book_events
                            );
                        }
                    }
                    _ => {}
                }
            }
        });
        self.clients.insert(
            name.clone(),
            ConnectedBot {
                client: client.clone(),
                username: username.clone(),
                mesadmin: format!("Joined {address} as {username}"),
                event_task,
                world_time,
                open_book_events,
            },
        );
        self.publish_status(format!("Joined {address} as {username} [{name}]"), None)
            .await;
        Ok(())
    }

    fn client_for_pearl(&self, pearl: &PearlEntry) -> Result<&Client> {
        let requested = clean_bot_name(&pearl.bot_name);
        if !requested.is_empty() {
            return self
                .clients
                .get(&requested)
                .map(|b| &b.client)
                .ok_or_else(|| anyhow!("bot '{requested}' is not connected"));
        }
        self.clients
            .values()
            .next()
            .map(|b| &b.client)
            .ok_or_else(|| anyhow!("no bot is connected; owner must press Connect Bot first"))
    }

    fn client_for_name(&self, bot_name: &str) -> Result<&Client> {
        let requested = clean_bot_name(bot_name);
        if !requested.is_empty() {
            return self
                .clients
                .get(&requested)
                .map(|b| &b.client)
                .ok_or_else(|| anyhow!("bot '{requested}' is not connected"));
        }
        self.clients
            .values()
            .next()
            .map(|b| &b.client)
            .ok_or_else(|| anyhow!("no bot is connected; owner must press Connect Bot first"))
    }

    fn connected_bot_for_name(&self, bot_name: &str) -> Result<&ConnectedBot> {
        let requested = clean_bot_name(bot_name);
        if !requested.is_empty() {
            return self
                .clients
                .get(&requested)
                .ok_or_else(|| anyhow!("bot '{requested}' is not connected"));
        }
        self.clients
            .values()
            .next()
            .ok_or_else(|| anyhow!("no bot is connected; owner must press Connect Bot first"))
    }

    async fn pull(&mut self, pearl: PearlEntry, return_waypoint: Option<WaypointEntry>) -> Result<()> {
        self.halt_if_requested("pearl pull")?;
        let client = self.client_for_pearl(&pearl)?.clone();
        let pos = BlockPos::new(pearl.x, pearl.y, pearl.z);
        println!(
            "Pull requested: player='{}' label='{}' bot='{}' target=({}, {}, {})",
            pearl.player, pearl.label, pearl.bot_name, pearl.x, pearl.y, pearl.z
        );
        self.publish_status(
            format!(
                "Walking to pearl location for {} / {} at {}, {}, {}",
                pearl.player, pearl.label, pearl.x, pearl.y, pearl.z
            ),
            None,
        )
        .await;
        self.walk_xz(&client, pos, 3.0, "pearl stasis block")
            .await?;
        client.look_at(pos.center());
        client.wait_ticks(4).await;
        client.start_use_item();
        client.wait_ticks(2).await;
        client.block_interact(pos);
        client.wait_ticks(4).await;
        println!(
            "Pull interaction sent for '{}' / '{}' at {}, {}, {}",
            pearl.player, pearl.label, pearl.x, pearl.y, pearl.z
        );
        self.publish_status(
            format!(
                "Pull interaction sent for {} / {} at {}, {}, {}",
                pearl.player, pearl.label, pearl.x, pearl.y, pearl.z
            ),
            None,
        )
        .await;

        if let Some(mut home) = return_waypoint {
            if home.bot_name.trim().is_empty() {
                home.bot_name = pearl.bot_name.clone();
            }
            if home.bot_name.trim().is_empty() {
                // If the pearl did not name a bot either, keep the old behaviour: first connected bot.
                println!(
                    "Auto-walk return waypoint '{}' has no bot name; using first connected bot",
                    home.label
                );
            }
            println!(
                "Auto-walk after pearl pull enabled: returning bot '{}' to waypoint '{}' at {}, {}, {}",
                if home.bot_name.trim().is_empty() { "first connected bot" } else { home.bot_name.trim() },
                home.label, home.x, home.y, home.z
            );
            self.publish_status(
                format!(
                    "Pearl pull complete; returning to waypoint '{}' at {}, {}, {}",
                    home.label, home.x, home.y, home.z
                ),
                None,
            )
            .await;
            self.walk_to_waypoint(home).await?;
        }

        Ok(())
    }

    async fn throw_stasis(&mut self, pearl: PearlEntry) -> Result<()> {
        let client = self.client_for_pearl(&pearl)?.clone();
        let inventory_slot = pearl.inventory_slot.min(35) as usize;
        let item_name = pearl.item_name.trim();
        if item_name.is_empty() {
            return Err(anyhow!(
                "no named item configured for this player/stasis entry"
            ));
        }
        println!(
            "ThrowStasis requested: player='{}' item='{}' inventory_slot={} bot='{}' label='{}'",
            pearl.player, item_name, inventory_slot, pearl.bot_name, pearl.label
        );

        let inventory = client.open_inventory()?.ok_or_else(|| {
            anyhow!("could not open the bot inventory; close any open container and try again")
        })?;
        let menu = inventory.menu()?.ok_or_else(|| {
            anyhow!("inventory menu was not available yet; try again after the bot fully loads")
        })?;

        let protocol_slot = if inventory_slot <= 8 {
            *menu.hotbar_slots_range().start() + inventory_slot
        } else {
            *menu.player_slots_without_hotbar_range().start() + (inventory_slot - 9)
        };

        inventory.click(ThrowClick::Single {
            slot: protocol_slot as u16,
        });
        client.wait_ticks(3).await;
        println!(
            "ThrowStasis drop sent for '{}' item '{}' from inventory slot {} (protocol slot {})",
            pearl.player, item_name, inventory_slot, protocol_slot
        );
        self.publish_status(
            format!(
                "ThrowStasis drop sent for {} / {} from inventory slot {}",
                pearl.player, item_name, inventory_slot
            ),
            None,
        )
        .await;
        Ok(())
    }

    async fn walk_to_waypoint(
        &mut self,
        waypoint: WaypointEntry,
    ) -> Result<WalkToWaypointResponse> {
        let client = self.client_for_name(&waypoint.bot_name)?.clone();
        let pos = BlockPos::new(waypoint.x, waypoint.y, waypoint.z);
        let bot_name = if waypoint.bot_name.trim().is_empty() {
            "first connected bot".to_string()
        } else {
            clean_bot_name(&waypoint.bot_name)
        };
        println!(
            "Waypoint walk requested: bot='{bot_name}' label='{}' target=({}, {}, {})",
            waypoint.label, waypoint.x, waypoint.y, waypoint.z
        );
        if waypoint.y == 0 {
            println!(
                "Waypoint '{}' has Y=0; ignoring Y and walking by X/Z only",
                waypoint.label
            );
        }
        self.publish_status(
            format!(
                "Walking bot {bot_name} to waypoint '{}' at {}, {}, {}",
                waypoint.label, waypoint.x, waypoint.y, waypoint.z
            ),
            None,
        )
        .await;
        self.walk_xz(&client, pos, 0.8, &waypoint.label).await?;
        client.look_at(pos.center());
        client.wait_ticks(3).await;
        let mesadmin = format!(
            "Bot {bot_name} walked to waypoint '{}' at {}, {}, {}",
            waypoint.label, waypoint.x, waypoint.y, waypoint.z
        );
        self.publish_status(mesadmin.clone(), None).await;
        Ok(WalkToWaypointResponse {
            mesadmin,
            bot_name,
            label: waypoint.label,
            x: waypoint.x,
            y: waypoint.y,
            z: waypoint.z,
        })
    }

    async fn walk_xz(
        &self,
        client: &Client,
        pos: BlockPos,
        radius: f64,
        label: &str,
    ) -> Result<()> {
        self.halt_if_requested(label)?;
        let current = client
            .get_component::<Position>()
            .map(|p| **p)
            .ok_or_else(|| anyhow!("bot has no position yet; wait a few seconds after joining"))?;
        let dx = current.x - f64::from(pos.x);
        let dz = current.z - f64::from(pos.z);
        let xz_distance = (dx * dx + dz * dz).sqrt();
        println!(
            "Waypoint '{label}' X/Z distance before walking: {:.2} blocks",
            xz_distance
        );
        auto_eat_if_hungry(client, label, true).await;
        if xz_distance <= radius {
            println!("Waypoint '{label}' already within radius {radius:.2}; no walking needed");
            return Ok(());
        }
        if dx.abs() > 3000.0 || dz.abs() > 3000.0 {
            return Err(anyhow!(
                "'{label}' is outside the 3000x3000 walking range from the bot (dx {:.0}, dz {:.0}). Add an intermediate waypoint closer to the route.",
                dx.abs(),
                dz.abs()
            ));
        }
        if xz_distance > 128.0 {
            println!(
                "Waypoint '{label}' is {:.0} blocks away; using staged long-distance walking",
                xz_distance
            );
            return self
                .walk_xz_staged(client, pos, radius, label, xz_distance)
                .await;
        }
        println!(
            "Planning loaded-world path for waypoint '{label}' to X/Z ({}, {})",
            pos.x, pos.z
        );
        client.force_stop_pathfinding();
        self.steer_walk_xz(client, pos, radius, label).await
    }

    async fn walk_xz_staged(
        &self,
        client: &Client,
        pos: BlockPos,
        radius: f64,
        label: &str,
        initial_distance: f64,
    ) -> Result<()> {
        let max_stage = 96.0;
        let max_stages = 96;
        let mut stage = 0;
        loop {
            self.halt_if_requested(label)?;
            let now = client
                .get_component::<Position>()
                .map(|p| **p)
                .ok_or_else(|| anyhow!("bot lost position during staged walking"))?;
            let target_x = f64::from(pos.x) + 0.5;
            let target_z = f64::from(pos.z) + 0.5;
            let dx = target_x - now.x;
            let dz = target_z - now.z;
            let remaining = (dx * dx + dz * dz).sqrt();
            if remaining <= radius + 0.35 {
                println!(
                    "Staged walk '{label}' reached final target after {stage} stage(s); remaining {:.2}",
                    remaining
                );
                return Ok(());
            }
            if stage >= max_stages {
                return Err(anyhow!(
                    "staged walk to '{label}' stopped after {max_stages} stage(s), still {:.1} blocks away",
                    remaining
                ));
            }
            let final_stage = remaining <= max_stage;
            let ratio = if final_stage {
                1.0
            } else {
                max_stage / remaining
            };
            let stage_x = if final_stage {
                pos.x
            } else {
                (now.x + dx * ratio).floor() as i32
            };
            let stage_z = if final_stage {
                pos.z
            } else {
                (now.z + dz * ratio).floor() as i32
            };
            let stage_y = if final_stage && pos.y != 0 {
                pos.y
            } else {
                now.y.floor() as i32
            };
            let stage_pos = BlockPos::new(stage_x, stage_y, stage_z);
            stage += 1;
            println!(
                "Staged walk '{label}': stage {stage}, target=({}, {}, {}), remaining {:.1}/{:.1}",
                stage_pos.x, stage_pos.y, stage_pos.z, remaining, initial_distance
            );
            self.publish_status(
                format!(
                    "Walking '{label}' stage {stage}: {:.0} blocks remaining",
                    remaining
                ),
                None,
            )
            .await;
            self.steer_walk_xz(
                client,
                stage_pos,
                if final_stage { radius } else { 3.25 },
                &format!("{label} stage {stage}"),
            )
            .await?;
            client.wait_ticks(4).await;
        }
    }

    async fn steer_walk_xz(
        &self,
        client: &Client,
        pos: BlockPos,
        radius: f64,
        label: &str,
    ) -> Result<()> {
        client.force_stop_pathfinding();
        client.wait_ticks(2).await;
        let start = client
            .get_component::<Position>()
            .map(|p| **p)
            .ok_or_else(|| anyhow!("bot has no position for planned walking"))?;
        println!(
            "Planned walking waypoint '{label}' from ({:.2}, {:.2}, {:.2}) to X/Z ({}, {})",
            start.x, start.y, start.z, pos.x, pos.z
        );

        let mut planned_any = false;
        let mut blocked_replans = 0;
        let mut blocked_columns: Vec<(i32, i32, i32)> = Vec::new();
        let mut last_round_pos = start;
        for round in 0..48 {
            self.halt_if_requested(label)?;
            blocked_columns.retain(|(_, _, until_round)| *until_round > round);
            let now = client
                .get_component::<Position>()
                .map(|p| **p)
                .ok_or_else(|| anyhow!("bot lost position data during planned walking"))?;
            let jump_from_last =
                ((now.x - last_round_pos.x).powi(2) + (now.z - last_round_pos.z).powi(2)).sqrt();
            if jump_from_last > 32.0 {
                client.walk(WalkDirection::None);
                let _ = client.set_jumping(false);
                self.set_movement_state(client, WalkDirection::None, false);
                return Err(anyhow!(
                    "walk to '{label}' stopped because the bot suddenly moved {:.1} blocks from ({:.1}, {:.1}, {:.1}) to ({:.1}, {:.1}, {:.1}). This looks like death, respawn, teleport, or server rubber-banding; start the walk again from the new position.",
                    jump_from_last,
                    last_round_pos.x,
                    last_round_pos.y,
                    last_round_pos.z,
                    now.x,
                    now.y,
                    now.z
                ));
            }
            let target_x = f64::from(pos.x) + 0.5;
            let target_z = f64::from(pos.z) + 0.5;
            let remaining = ((target_x - now.x).powi(2) + (target_z - now.z).powi(2)).sqrt();
            if remaining <= radius + 0.35 {
                client.walk(WalkDirection::None);
                let _ = client.set_jumping(false);
                self.set_movement_state(client, WalkDirection::None, false);
                println!(
                    "Planned ground path reached waypoint '{label}' after {round} plan round(s); remaining {:.2}",
                    remaining
                );
                return Ok(());
            }

            let Some(path) = self.find_planned_ground_path(client, pos, label, &blocked_columns)?
            else {
                if self
                    .advance_toward_unloaded_frontier(client, pos, label, round + 1)
                    .await?
                {
                    planned_any = true;
                    if let Some(after_frontier) = client.get_component::<Position>().map(|p| **p) {
                        last_round_pos = after_frontier;
                    }
                    continue;
                }
                if planned_any {
                    break;
                }
                return Err(anyhow!(
                    "could not draw a clear loaded-world path to '{label}' from the bot's current position, and frontier advance could not move the bot. Move the bot/waypoint closer, or clear terrain between them."
                ));
            };
            planned_any = true;
            println!(
                "Planned ground path '{label}': round {}, following {} clear step(s) before replanning",
                round + 1,
                path.len().saturating_sub(1)
            );
            if !self.follow_local_room_path(client, &path, label).await? {
                blocked_replans += 1;
                let current_after_block = client
                    .get_component::<Position>()
                    .map(|p| **p)
                    .unwrap_or(now);
                println!(
                    "Planned path toward '{label}' became blocked or vertically invalid on round {}; replanning from actual bot position ({:.2}, {:.2}, {:.2}) instead of pushing the old path",
                    round + 1,
                    current_after_block.x,
                    current_after_block.y,
                    current_after_block.z
                );
                let blocked_x = current_after_block.x.floor() as i32;
                let blocked_z = current_after_block.z.floor() as i32;
                blocked_columns.push((blocked_x, blocked_z, round + 12));
                if let Some(next_step) = path.get(1) {
                    blocked_columns.push((next_step.x, next_step.z, round + 12));
                }
                println!(
                    "Planned path '{label}' marked ({blocked_x}, {blocked_z}) as temporarily blocked so the next plan searches around it"
                );
                self.publish_status(
                    format!(
                        "Walking waypoint '{label}': replanning from {:.1}, {:.1}, {:.1}",
                        current_after_block.x, current_after_block.y, current_after_block.z
                    ),
                    None,
                )
                .await;
                if blocked_replans > 8 {
                    return Err(anyhow!(
                        "planned path toward '{label}' became blocked too many times; stopped instead of pushing a wall"
                    ));
                }
                client.walk(WalkDirection::None);
                let _ = client.set_jumping(false);
                self.set_movement_state(client, WalkDirection::None, false);
                client.wait_ticks(4).await;
                if let Some(after_replan_pause) = client.get_component::<Position>().map(|p| **p) {
                    last_round_pos = after_replan_pause;
                }
                continue;
            }
            blocked_replans = 0;
            if let Some(after_path) = client.get_component::<Position>().map(|p| **p) {
                last_round_pos = after_path;
            }
        }

        let after_plans = client
            .get_component::<Position>()
            .map(|p| **p)
            .unwrap_or(start);
        let target_x = f64::from(pos.x) + 0.5;
        let target_z = f64::from(pos.z) + 0.5;
        let after_remaining =
            ((target_x - after_plans.x).powi(2) + (target_z - after_plans.z).powi(2)).sqrt();
        if after_remaining <= radius + 0.35 {
            client.walk(WalkDirection::None);
            let _ = client.set_jumping(false);
            self.set_movement_state(client, WalkDirection::None, false);
            println!(
                "Planned ground path reached waypoint '{label}' after final recheck; remaining {:.2}",
                after_remaining
            );
            return Ok(());
        }
        if planned_any {
            return Err(anyhow!(
                "planned walking stopped {:.2} blocks from '{label}' because no further clear loaded-world path was found",
                after_remaining
            ));
        }

        let mut best_remaining = f64::MAX;
        let mut stalled_checks = 0;
        let mut detour_left = false;
        let mut exit_commit_until = 0;
        let mut exit_commit_yaw = 0.0;
        let mut avoid_zones: Vec<(f64, f64, i32)> = Vec::new();
        for tick in 0..900 {
            self.halt_if_requested(label)?;
            avoid_zones.retain(|(_, _, until)| *until > tick);
            let now = client
                .get_component::<Position>()
                .map(|p| **p)
                .ok_or_else(|| anyhow!("bot lost position data during steering fallback"))?;
            let target_x = f64::from(pos.x) + 0.5;
            let target_z = f64::from(pos.z) + 0.5;
            let dx = target_x - now.x;
            let dz = target_z - now.z;
            let remaining = (dx * dx + dz * dz).sqrt();
            let moved = ((now.x - start.x).powi(2) + (now.z - start.z).powi(2)).sqrt();
            if remaining <= radius {
                client.walk(WalkDirection::None);
                let _ = client.set_jumping(false);
                self.set_movement_state(client, WalkDirection::None, false);
                println!(
                    "Steering fallback reached waypoint '{label}' after {tick} tick(s); moved {:.2}, remaining {:.2}",
                    moved, remaining
                );
                return Ok(());
            }

            if tick % 20 == 0 {
                client.force_stop_pathfinding();
                if remaining + 0.35 < best_remaining {
                    best_remaining = remaining;
                    stalled_checks = 0;
                } else {
                    stalled_checks += 1;
                }
                if stalled_checks >= 4 {
                    detour_left = !detour_left;
                    if let Some(path) =
                        self.find_local_room_path(client, pos, label, &avoid_zones)?
                    {
                        let before_local = now;
                        let path_end = path.last().copied().unwrap_or(pos);
                        let path_end_x = f64::from(path_end.x) + 0.5;
                        let path_end_z = f64::from(path_end.z) + 0.5;
                        let before_path_distance = ((target_x - before_local.x).powi(2)
                            + (target_z - before_local.z).powi(2))
                        .sqrt();
                        let path_end_distance = ((target_x - path_end_x).powi(2)
                            + (target_z - path_end_z).powi(2))
                        .sqrt();
                        let path_progress = before_path_distance - path_end_distance;
                        let far_from_start = ((before_local.x - start.x).powi(2)
                            + (before_local.z - start.z).powi(2))
                        .sqrt()
                            > 24.0;
                        if far_from_start && path_progress < 1.0 {
                            println!(
                                "Steering fallback '{label}' rejected local room path because the bot is already clear of the first obstacle and that path would not progress toward the waypoint ({path_progress:.2} blocks)"
                            );
                        } else {
                            println!(
                            "Steering fallback '{label}' found a local room path with {} step(s); following it before retrying the far waypoint",
                            path.len().saturating_sub(1)
                        );
                            if self.follow_local_room_path(client, &path, label).await? {
                                if let Some(after_local) =
                                    client.get_component::<Position>().map(|p| **p)
                                {
                                    let before_distance = ((target_x - before_local.x).powi(2)
                                        + (target_z - before_local.z).powi(2))
                                    .sqrt();
                                    let after_distance = ((target_x - after_local.x).powi(2)
                                        + (target_z - after_local.z).powi(2))
                                    .sqrt();
                                    if after_distance > before_distance + 2.0 {
                                        let exit_yaw = yaw_between(
                                            before_local.x,
                                            before_local.z,
                                            after_local.x,
                                            after_local.z,
                                        );
                                        exit_commit_until = tick + 140;
                                        exit_commit_yaw = exit_yaw;
                                        avoid_zones.push((
                                            before_local.x,
                                            before_local.z,
                                            tick + 520,
                                        ));
                                        println!(
                                        "Steering fallback '{label}' is committing through the doorway/exit before re-aiming at the waypoint; local path temporarily increased remaining distance from {:.2} to {:.2}. Marking that doorway/wall area as temporary avoid-zone.",
                                        before_distance,
                                        after_distance
                                    );
                                    }
                                }
                                stalled_checks = 0;
                                best_remaining = f64::MAX;
                                continue;
                            }
                            println!(
                            "Steering fallback '{label}' could not complete the local room path; using wall recovery next"
                        );
                        }
                    }
                    println!(
                        "Steering fallback '{label}' is pushing into a wall near ({:.2}, {:.2}, {:.2}); backing away and wall-following {} before retrying",
                        now.x,
                        now.y,
                        now.z,
                        if detour_left { "left" } else { "right" }
                    );
                    self.escape_wall_for_waypoint(client, pos, label, detour_left)
                        .await?;
                    stalled_checks = 0;
                    best_remaining = f64::MAX;
                    continue;
                }
            }

            let target_yaw = if tick < exit_commit_until {
                exit_commit_yaw
            } else {
                let mut yaw = yaw_toward(now.x, now.z, pos);
                if let Some(tangent_yaw) = avoid_zone_tangent_yaw(now.x, now.z, pos, &avoid_zones) {
                    yaw = tangent_yaw;
                    if tick % 20 == 0 {
                        println!(
                            "Steering fallback '{label}': avoiding recently exited house/doorway zone while outside"
                        );
                    }
                }
                yaw
            };
            let _ = client.set_direction(target_yaw, 0.0);
            let direction = WalkDirection::Forward;
            self.set_movement_state(client, direction, true);
            client.walk(direction);
            self.sprint_if_food_reserve(client);
            let _ = client.set_jumping(false);
            if tick % 20 == 0 {
                if tick < exit_commit_until {
                    println!(
                        "Steering fallback '{label}': tick={tick}, exit-commit active, pos=({:.2}, {:.2}, {:.2}), moved_xz={:.2}, remaining_xz={:.2}",
                        now.x, now.y, now.z, moved, remaining
                    );
                } else {
                    println!(
                        "Steering fallback '{label}': tick={tick}, pos=({:.2}, {:.2}, {:.2}), moved_xz={:.2}, remaining_xz={:.2}",
                        now.x, now.y, now.z, moved, remaining
                    );
                }
            }
            client.wait_ticks(1).await;
        }

        client.walk(WalkDirection::None);
        let _ = client.set_jumping(false);
        self.set_movement_state(client, WalkDirection::None, false);
        let end = client
            .get_component::<Position>()
            .map(|p| **p)
            .unwrap_or(start);
        let moved = ((end.x - start.x).powi(2) + (end.z - start.z).powi(2)).sqrt();
        let target_x = f64::from(pos.x) + 0.5;
        let target_z = f64::from(pos.z) + 0.5;
        let remaining = ((target_x - end.x).powi(2) + (target_z - end.z).powi(2)).sqrt();
        Err(anyhow!(
            "steering fallback could not reach '{label}'. It moved {:.2} blocks and stopped {:.2} blocks from the target. The bot may be blocked, rubber-banded by the server, or missing movement permission.",
            moved,
            remaining
        ))
    }

    fn find_local_room_path(
        &self,
        client: &Client,
        pos: BlockPos,
        label: &str,
        avoid_zones: &[(f64, f64, i32)],
    ) -> Result<Option<Vec<BlockPos>>> {
        let now = client
            .get_component::<Position>()
            .map(|p| **p)
            .ok_or_else(|| anyhow!("bot lost position data before local room scan"))?;
        let start = (now.x.floor() as i32, now.z.floor() as i32);
        let y = now.y.floor() as i32;
        let radius = 18;
        let min_x = start.0 - radius;
        let max_x = start.0 + radius;
        let min_z = start.1 - radius;
        let max_z = start.1 + radius;
        let target_x = f64::from(pos.x) + 0.5;
        let target_z = f64::from(pos.z) + 0.5;
        let start_distance = ((target_x - now.x).powi(2) + (target_z - now.z).powi(2)).sqrt();
        let world = client.world().map_err(|e| anyhow!("world component unavailable: {e}"))?;
        let world = world.read();

        let is_passable = |x: i32, z: i32| -> bool {
            let foot = BlockPos::new(x, y, z);
            let head = BlockPos::new(x, y + 1, z);
            let floor = BlockPos::new(x, y - 1, z);
            let Some(foot_state) = world.get_block_state(foot) else {
                return false;
            };
            let Some(head_state) = world.get_block_state(head) else {
                return false;
            };
            let Some(floor_state) = world.get_block_state(floor) else {
                return false;
            };
            is_walkable_body_block(foot_state)
                && is_walkable_head_block(head_state)
                && !floor_state.is_air()
        };

        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut came_from: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
        queue.push_back(start);
        visited.insert(start);

        while let Some((x, z)) = queue.pop_front() {
            for (nx, nz) in [(x + 1, z), (x - 1, z), (x, z + 1), (x, z - 1)] {
                if nx < min_x || nx > max_x || nz < min_z || nz > max_z {
                    continue;
                }
                if visited.contains(&(nx, nz)) {
                    continue;
                }
                if !is_passable(nx, nz) {
                    continue;
                }
                visited.insert((nx, nz));
                came_from.insert((nx, nz), (x, z));
                queue.push_back((nx, nz));
            }
        }

        let mut best = None;
        let mut best_score = f64::MAX;
        let mut best_progress = f64::NEG_INFINITY;
        for &(x, z) in &visited {
            if (x, z) == start {
                continue;
            }
            let cell_x = f64::from(x) + 0.5;
            let cell_z = f64::from(z) + 0.5;
            let distance = ((target_x - cell_x).powi(2) + (target_z - cell_z).powi(2)).sqrt();
            let progress = start_distance - distance;
            let from_start = ((cell_x - now.x).powi(2) + (cell_z - now.z).powi(2)).sqrt();
            if from_start < 2.0 {
                continue;
            }
            let open_neighbors = [(x + 1, z), (x - 1, z), (x, z + 1), (x, z - 1)]
                .into_iter()
                .filter(|&(nx, nz)| {
                    nx >= min_x && nx <= max_x && nz >= min_z && nz <= max_z && is_passable(nx, nz)
                })
                .count() as f64;
            let boundary_bonus = if x == min_x || x == max_x || z == min_z || z == max_z {
                -16.0
            } else {
                0.0
            };
            let forward_bias = if progress > 0.0 {
                progress * 0.85
            } else {
                progress * 0.2
            };
            let avoid_penalty: f64 = avoid_zones
                .iter()
                .map(|(ax, az, _)| {
                    let avoid_distance = ((cell_x - *ax).powi(2) + (cell_z - *az).powi(2)).sqrt();
                    if avoid_distance < 22.0 {
                        (22.0 - avoid_distance) * 30.0
                    } else if avoid_distance < 32.0 {
                        (32.0 - avoid_distance) * 4.0
                    } else {
                        0.0
                    }
                })
                .sum();
            let score = distance - forward_bias - (from_start * 0.75) - (open_neighbors * 1.5)
                + boundary_bonus
                + avoid_penalty;
            if score < best_score {
                best_score = score;
                best_progress = progress;
                best = Some((x, z));
            }
        }

        let Some(mut cursor) = best else {
            println!(
                "Local room scan '{label}' found {} reachable floor cell(s), but none were far enough from the bot to use as a local step",
                visited.len()
            );
            return Ok(None);
        };

        let mut path = vec![cursor];
        while cursor != start {
            let Some(prev) = came_from.get(&cursor).copied() else {
                return Ok(None);
            };
            cursor = prev;
            path.push(cursor);
        }
        path.reverse();
        if path.len() > 14 {
            path.truncate(14);
        }
        println!(
            "Local room scan '{label}' selected path from ({}, {}) to ({}, {}) across {} reachable cell(s), projected progress {:.2} blocks",
            start.0,
            start.1,
            path.last().map(|p| p.0).unwrap_or(start.0),
            path.last().map(|p| p.1).unwrap_or(start.1),
            visited.len(),
            best_progress
        );
        Ok(Some(
            path.into_iter()
                .map(|(x, z)| BlockPos::new(x, y, z))
                .collect(),
        ))
    }

    fn find_planned_ground_path(
        &self,
        client: &Client,
        pos: BlockPos,
        label: &str,
        blocked_columns: &[(i32, i32, i32)],
    ) -> Result<Option<Vec<BlockPos>>> {
        let now = client
            .get_component::<Position>()
            .map(|p| **p)
            .ok_or_else(|| anyhow!("bot lost position data before planned path scan"))?;
        let start = (now.x.floor() as i32, now.z.floor() as i32);
        let base_y = now.y.floor() as i32;
        let target = (pos.x, pos.z);
        let target_x = f64::from(pos.x) + 0.5;
        let target_z = f64::from(pos.z) + 0.5;
        let start_distance = ((target_x - now.x).powi(2) + (target_z - now.z).powi(2)).sqrt();
        let search_radius = start_distance.ceil().min(96.0).max(32.0) as i32;
        let min_x = start.0 - search_radius;
        let max_x = start.0 + search_radius;
        let min_z = start.1 - search_radius;
        let max_z = start.1 + search_radius;
        let world = client.world().map_err(|e| anyhow!("world component unavailable: {e}"))?;
        let world = world.read();

        let is_temporarily_blocked = |x: i32, z: i32| -> bool {
            blocked_columns
                .iter()
                .any(|(bx, bz, _)| (x - *bx).abs() <= 1 && (z - *bz).abs() <= 1)
        };

        let walkable_y = |x: i32, z: i32| -> Option<i32> {
            if is_temporarily_blocked(x, z) && (x, z) != start {
                return None;
            }
            let mut best_y = None;
            let mut best_score = i32::MAX;
            for y in (base_y - 4)..=(base_y + 4) {
                let foot = BlockPos::new(x, y, z);
                let head = BlockPos::new(x, y + 1, z);
                let floor = BlockPos::new(x, y - 1, z);
                let foot_state = world.get_block_state(foot)?;
                let head_state = world.get_block_state(head)?;
                let floor_state = world.get_block_state(floor)?;
                if is_walkable_body_block(foot_state)
                    && is_walkable_head_block(head_state)
                    && !floor_state.is_air()
                {
                    let score = (y - base_y).abs() * 2 + if y < base_y { 1 } else { 0 };
                    if score < best_score {
                        best_score = score;
                        best_y = Some(y);
                    }
                }
            }
            best_y
        };

        if walkable_y(start.0, start.1).is_none() {
            println!(
                "Planned ground path '{label}' could not start because the bot's current column was not walkable in loaded world data"
            );
            return Ok(None);
        }

        let mut open = vec![start];
        let mut open_set = HashSet::new();
        let mut closed = HashSet::new();
        let mut came_from: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
        let mut g_score: HashMap<(i32, i32), f64> = HashMap::new();
        open_set.insert(start);
        g_score.insert(start, 0.0);

        let mut best = start;
        let mut best_distance = start_distance;
        let mut iterations = 0usize;

        while !open.is_empty() && iterations < 12_000 {
            iterations += 1;
            let mut best_open_idx = 0usize;
            let mut best_open_score = f64::MAX;
            for (idx, node) in open.iter().enumerate() {
                let g = *g_score.get(node).unwrap_or(&f64::MAX);
                let h = (((target.0 - node.0) as f64).powi(2)
                    + ((target.1 - node.1) as f64).powi(2))
                .sqrt();
                let score = g + h;
                if score < best_open_score {
                    best_open_score = score;
                    best_open_idx = idx;
                }
            }
            let current = open.swap_remove(best_open_idx);
            open_set.remove(&current);
            if !closed.insert(current) {
                continue;
            }

            let current_x = f64::from(current.0) + 0.5;
            let current_z = f64::from(current.1) + 0.5;
            let current_distance =
                ((target_x - current_x).powi(2) + (target_z - current_z).powi(2)).sqrt();
            if current_distance < best_distance {
                best_distance = current_distance;
                best = current;
            }
            if current == target || current_distance <= 1.2 {
                best = current;
                break;
            }

            let current_y = walkable_y(current.0, current.1).unwrap_or(base_y);
            for (nx, nz, step_cost) in [
                (current.0 + 1, current.1, 1.0),
                (current.0 - 1, current.1, 1.0),
                (current.0, current.1 + 1, 1.0),
                (current.0, current.1 - 1, 1.0),
                (current.0 + 1, current.1 + 1, 1.45),
                (current.0 + 1, current.1 - 1, 1.45),
                (current.0 - 1, current.1 + 1, 1.45),
                (current.0 - 1, current.1 - 1, 1.45),
            ] {
                if nx < min_x || nx > max_x || nz < min_z || nz > max_z {
                    continue;
                }
                if closed.contains(&(nx, nz)) {
                    continue;
                }
                let Some(next_y) = walkable_y(nx, nz) else {
                    continue;
                };
                if (next_y - current_y).abs() > 1 {
                    continue;
                }
                if nx != current.0 && nz != current.1 {
                    if walkable_y(nx, current.1).is_none() || walkable_y(current.0, nz).is_none() {
                        continue;
                    }
                }
                let height_penalty = f64::from((next_y - current_y).abs()) * 1.8;
                let tentative_g =
                    g_score.get(&current).copied().unwrap_or(f64::MAX) + step_cost + height_penalty;
                let neighbor = (nx, nz);
                if tentative_g < g_score.get(&neighbor).copied().unwrap_or(f64::MAX) {
                    came_from.insert(neighbor, current);
                    g_score.insert(neighbor, tentative_g);
                    if open_set.insert(neighbor) {
                        open.push(neighbor);
                    }
                }
            }
        }

        let progress = start_distance - best_distance;
        if best == start || progress < 1.0 {
            let mut escape = None;
            let mut escape_score = f64::NEG_INFINITY;
            for &(x, z) in &closed {
                if (x, z) == start {
                    continue;
                }
                let cell_x = f64::from(x) + 0.5;
                let cell_z = f64::from(z) + 0.5;
                let from_start = ((cell_x - now.x).powi(2) + (cell_z - now.z).powi(2)).sqrt();
                if from_start < 4.0 {
                    continue;
                }
                let distance = ((target_x - cell_x).powi(2) + (target_z - cell_z).powi(2)).sqrt();
                let candidate_progress = start_distance - distance;
                let open_neighbors = [
                    (x + 1, z),
                    (x - 1, z),
                    (x, z + 1),
                    (x, z - 1),
                    (x + 1, z + 1),
                    (x + 1, z - 1),
                    (x - 1, z + 1),
                    (x - 1, z - 1),
                ]
                .into_iter()
                .filter(|&(nx, nz)| {
                    nx >= min_x
                        && nx <= max_x
                        && nz >= min_z
                        && nz <= max_z
                        && walkable_y(nx, nz).is_some()
                })
                .count() as f64;
                let boundary_bonus =
                    if x <= min_x + 2 || x >= max_x - 2 || z <= min_z + 2 || z >= max_z - 2 {
                        12.0
                    } else {
                        0.0
                    };
                let score = (open_neighbors * 5.0)
                    + (from_start.min(28.0) * 1.4)
                    + (candidate_progress * 0.8)
                    + boundary_bonus;
                if score > escape_score {
                    escape_score = score;
                    escape = Some((x, z, candidate_progress, distance, from_start));
                }
            }

            if let Some((x, z, candidate_progress, distance, from_start)) = escape {
                best = (x, z);
                best_distance = distance;
                println!(
                    "Planned ground path '{label}' found no direct progress, so it selected a clear-space route from ({}, {}) to ({}, {}) before replanning. Projected progress {:.2} blocks, distance from start {:.2}, remaining {:.2}, scanned {} node(s).",
                    start.0,
                    start.1,
                    best.0,
                    best.1,
                    candidate_progress,
                    from_start,
                    best_distance,
                    iterations
                );
            } else {
                println!(
                    "Planned ground path '{label}' scanned {iterations} node(s) but found no clear useful progress from ({}, {}). Best loaded cell was ({}, {}), progress {:.2} blocks, remaining {:.2} blocks.",
                    start.0,
                    start.1,
                    best.0,
                    best.1,
                    progress.max(0.0),
                    best_distance
                );
                return Ok(None);
            }
        }

        let mut cursor = best;
        let mut path = vec![cursor];
        while cursor != start {
            let Some(prev) = came_from.get(&cursor).copied() else {
                return Ok(None);
            };
            cursor = prev;
            path.push(cursor);
        }
        path.reverse();
        if path.len() > 64 {
            path.truncate(64);
        }
        println!(
            "Planned ground path '{label}' drew path from ({}, {}) to ({}, {}), progress {:.2} blocks, scanned {} node(s)",
            start.0,
            start.1,
            path.last().map(|p| p.0).unwrap_or(start.0),
            path.last().map(|p| p.1).unwrap_or(start.1),
            progress,
            iterations
        );
        Ok(Some(
            path.into_iter()
                .map(|(x, z)| {
                    let y = walkable_y(x, z).unwrap_or(base_y);
                    BlockPos::new(x, y, z)
                })
                .collect(),
        ))
    }

    async fn advance_toward_unloaded_frontier(
        &self,
        client: &Client,
        pos: BlockPos,
        label: &str,
        round: i32,
    ) -> Result<bool> {
        client.force_stop_pathfinding();
        client.walk(WalkDirection::None);
        let _ = client.set_jumping(false);
        self.set_movement_state(client, WalkDirection::None, false);
        client.wait_ticks(2).await;
        let start = client
            .get_component::<Position>()
            .map(|p| **p)
            .ok_or_else(|| anyhow!("bot lost position before unloaded frontier advance"))?;
        let target_x = f64::from(pos.x) + 0.5;
        let target_z = f64::from(pos.z) + 0.5;
        let start_remaining = ((target_x - start.x).powi(2) + (target_z - start.z).powi(2)).sqrt();
        println!(
            "Frontier advance '{label}': no loaded path on round {round}; nudging toward unloaded chunks from ({:.2}, {:.2}, {:.2}), remaining {:.2}",
            start.x, start.y, start.z, start_remaining
        );
        self.publish_status(
            format!(
                "Walking waypoint '{label}': loading chunks ahead at {:.1}, {:.1}, {:.1}",
                start.x, start.y, start.z
            ),
            None,
        )
        .await;

        let mut best_remaining = start_remaining;
        let mut best_moved: f64 = 0.0;
        let mut stale_ticks = 0;
        let mut last_seen = start;
        for tick in 0..42 {
            let now = client
                .get_component::<Position>()
                .map(|p| **p)
                .ok_or_else(|| anyhow!("bot lost position during unloaded frontier advance"))?;
            let snap = ((now.x - last_seen.x).powi(2) + (now.z - last_seen.z).powi(2)).sqrt();
            if snap > 32.0 {
                client.walk(WalkDirection::None);
                let _ = client.set_jumping(false);
                self.set_movement_state(client, WalkDirection::None, false);
                return Err(anyhow!(
                    "frontier advance for '{label}' stopped because the bot suddenly moved {:.1} blocks from ({:.1}, {:.1}, {:.1}) to ({:.1}, {:.1}, {:.1}). This looks like death, respawn, teleport, or server rubber-banding.",
                    snap,
                    last_seen.x,
                    last_seen.y,
                    last_seen.z,
                    now.x,
                    now.y,
                    now.z
                ));
            }
            last_seen = now;
            let remaining = ((target_x - now.x).powi(2) + (target_z - now.z).powi(2)).sqrt();
            let moved = ((now.x - start.x).powi(2) + (now.z - start.z).powi(2)).sqrt();
            if remaining + 0.04 < best_remaining || moved > best_moved + 0.04 {
                best_remaining = best_remaining.min(remaining);
                best_moved = best_moved.max(moved);
                stale_ticks = 0;
            } else {
                stale_ticks += 1;
            }
            if tick % 10 == 0 {
                println!(
                    "Frontier advance '{label}': tick={tick}, pos=({:.2}, {:.2}, {:.2}), moved_xz={:.2}, remaining_xz={:.2}",
                    now.x, now.y, now.z, moved, remaining
                );
            }
            if moved >= 6.0 || start_remaining - remaining >= 5.0 {
                break;
            }
            if stale_ticks > 20 && moved < 0.5 {
                break;
            }
            let _ = client.set_direction(yaw_toward(now.x, now.z, pos), 0.0);
            self.set_movement_state(client, WalkDirection::Forward, true);
            client.walk(WalkDirection::Forward);
            self.sprint_if_food_reserve(client);
            let _ = client.set_jumping(stale_ticks > 8 && tick % 16 < 7);
            client.wait_ticks(1).await;
        }

        client.walk(WalkDirection::None);
        let _ = client.set_jumping(false);
        self.set_movement_state(client, WalkDirection::None, false);
        client.wait_ticks(4).await;
        let end = client
            .get_component::<Position>()
            .map(|p| **p)
            .unwrap_or(start);
        let moved = ((end.x - start.x).powi(2) + (end.z - start.z).powi(2)).sqrt();
        let end_remaining = ((target_x - end.x).powi(2) + (target_z - end.z).powi(2)).sqrt();
        let useful = moved >= 0.75 || end_remaining + 0.5 < start_remaining;
        println!(
            "Frontier advance '{label}': ended at ({:.2}, {:.2}, {:.2}), moved_xz={:.2}, remaining_xz={:.2}, useful={}",
            end.x, end.y, end.z, moved, end_remaining, useful
        );
        Ok(useful)
    }

    async fn follow_local_room_path(
        &self,
        client: &Client,
        path: &[BlockPos],
        label: &str,
    ) -> Result<bool> {
        if path.len() <= 1 {
            return Ok(false);
        }
        client.force_stop_pathfinding();
        let mut last_seen = client
            .get_component::<Position>()
            .map(|p| **p)
            .ok_or_else(|| anyhow!("bot lost position before following local room path"))?;
        let mut handled_pasadmins: HashSet<(i32, i32, i32)> = HashSet::new();
        for (idx, step) in path.iter().enumerate().skip(1) {
            let mut reached = false;
            let mut best_distance = f64::MAX;
            let mut stale_ticks = 0;
            for tick in 0..55 {
                self.halt_if_requested(label)?;
                if tick % 10 == 0 {
                    client.force_stop_pathfinding();
                }
                if tick % 40 == 0 {
                    auto_eat_if_hungry(client, label, true).await;
                }
                let now = client
                    .get_component::<Position>()
                    .map(|p| **p)
                    .ok_or_else(|| {
                        anyhow!("bot lost position data while following local room path")
                    })?;
                let snap = ((now.x - last_seen.x).powi(2) + (now.z - last_seen.z).powi(2)).sqrt();
                if snap > 32.0 {
                    client.walk(WalkDirection::None);
                    let _ = client.set_jumping(false);
                    self.set_movement_state(client, WalkDirection::None, false);
                    return Err(anyhow!(
                        "walk to '{label}' stopped because the bot suddenly moved {:.1} blocks from ({:.1}, {:.1}, {:.1}) to ({:.1}, {:.1}, {:.1}). This looks like death, respawn, teleport, or server rubber-banding; start the walk again from the new position.",
                        snap,
                        last_seen.x,
                        last_seen.y,
                        last_seen.z,
                        now.x,
                        now.y,
                        now.z
                    ));
                }
                last_seen = now;
                if tick % 10 == 0 {
                    self.publish_status(
                        format!(
                            "Walking waypoint '{label}' step {idx}/{} at {:.1}, {:.1}, {:.1}",
                            path.len() - 1,
                            now.x,
                            now.y,
                            now.z
                        ),
                        None,
                    )
                    .await;
                    let push_target = path
                        .get(idx + 2)
                        .copied()
                        .or_else(|| path.get(idx + 1).copied())
                        .unwrap_or(*step);
                    if self
                        .open_pasadmin_for_step(
                            client,
                            *step,
                            push_target,
                            label,
                            &mut handled_pasadmins,
                        )
                        .await
                    {
                        reached = true;
                        break;
                    }
                }
                let target_x = f64::from(step.x) + 0.5;
                let target_z = f64::from(step.z) + 0.5;
                let distance = ((target_x - now.x).powi(2) + (target_z - now.z).powi(2)).sqrt();
                let vertical_gap = f64::from(step.y) - now.y;
                if vertical_gap > 2.25 && distance < 2.25 {
                    println!(
                        "Local room path '{label}' is {:.2} blocks below step {idx}/{}; trying vertical recovery before replanning.",
                        vertical_gap,
                        path.len() - 1
                    );
                    if self
                        .recover_vertical_gap(client, *step, label, idx, path.len() - 1)
                        .await?
                    {
                        stale_ticks = 0;
                        best_distance = f64::MAX;
                        continue;
                    }
                    println!(
                        "Local room path '{label}' abandoned step {idx}/{} because vertical recovery could not reach the planned step. Replanning from actual terrain instead.",
                        path.len() - 1
                    );
                    break;
                }
                if distance <= 0.55 || (distance <= 0.85 && vertical_gap <= 1.75) {
                    reached = true;
                    break;
                }
                if distance + 0.08 < best_distance {
                    best_distance = distance;
                    stale_ticks = 0;
                } else {
                    stale_ticks += 1;
                }
                if stale_ticks > 18 && vertical_gap > 1.0 && distance < 1.85 {
                    println!(
                        "Local room path '{label}' is stuck {:.2} blocks below step {idx}/{} while close to it; trying vertical recovery.",
                        vertical_gap,
                        path.len() - 1
                    );
                    if self
                        .recover_vertical_gap(client, *step, label, idx, path.len() - 1)
                        .await?
                    {
                        stale_ticks = 0;
                        best_distance = f64::MAX;
                        continue;
                    }
                }
                if stale_ticks > 28 {
                    println!(
                        "Local room path '{label}' stalled on step {idx}/{} near ({:.2}, {:.2}, {:.2}); target step=({}, {}, {})",
                        path.len() - 1,
                        now.x,
                        now.y,
                        now.z,
                        step.x,
                        step.y,
                        step.z
                    );
                    break;
                }
                let _ = client.set_direction(yaw_toward(now.x, now.z, *step), 0.0);
                self.set_movement_state(client, WalkDirection::Forward, true);
                client.walk(WalkDirection::Forward);
                self.sprint_if_food_reserve(client);
                let current_block_y = now.y.floor() as i32;
                let step_up = step.y > current_block_y && step.y - current_block_y <= 1;
                let swim_or_climb = f64::from(step.y) - now.y > 0.65;
                let close_and_stuck = distance < 1.45 && stale_ticks > 8;
                let _ = client.set_jumping(step_up || swim_or_climb || close_and_stuck);
                if tick % 10 == 0 && (step_up || swim_or_climb || close_and_stuck) {
                    println!(
                        "Local room path '{label}': jump/swim assist on step {idx}/{} from y {:.2} toward step y {}",
                        path.len() - 1,
                        now.y,
                        step.y
                    );
                }
                client.wait_ticks(1).await;
            }
            if !reached {
                client.walk(WalkDirection::None);
                let _ = client.set_jumping(false);
                self.set_movement_state(client, WalkDirection::None, false);
                return Ok(false);
            }
            if idx % 4 == 0 || idx + 1 == path.len() {
                if let Some(now) = client.get_component::<Position>().map(|p| **p) {
                    println!(
                        "Local room path '{label}': reached step {idx}/{} at ({:.2}, {:.2}, {:.2})",
                        path.len() - 1,
                        now.x,
                        now.y,
                        now.z
                    );
                }
            }
        }
        client.walk(WalkDirection::None);
        let _ = client.set_jumping(false);
        self.set_movement_state(client, WalkDirection::None, false);
        Ok(true)
    }

    async fn recover_vertical_gap(
        &self,
        client: &Client,
        step: BlockPos,
        label: &str,
        idx: usize,
        total: usize,
    ) -> Result<bool> {
        client.force_stop_pathfinding();
        let start = client
            .get_component::<Position>()
            .map(|p| **p)
            .ok_or_else(|| anyhow!("bot lost position before vertical recovery"))?;
        let target_x = f64::from(step.x) + 0.5;
        let target_z = f64::from(step.z) + 0.5;
        let mut best_gap = f64::from(step.y) - start.y;
        let mut best_y = start.y;
        let mut stale_ticks = 0;
        println!(
            "Vertical recovery '{label}': step {idx}/{total}, start=({:.2}, {:.2}, {:.2}), target step=({}, {}, {})",
            start.x, start.y, start.z, step.x, step.y, step.z
        );

        for tick in 0..100 {
            let now = client
                .get_component::<Position>()
                .map(|p| **p)
                .ok_or_else(|| anyhow!("bot lost position during vertical recovery"))?;
            let gap = f64::from(step.y) - now.y;
            let distance = ((target_x - now.x).powi(2) + (target_z - now.z).powi(2)).sqrt();
            if gap <= 1.35 || (gap <= 1.85 && distance > 0.8) {
                println!(
                    "Vertical recovery '{label}' regained step {idx}/{total}: pos=({:.2}, {:.2}, {:.2}), gap={:.2}",
                    now.x, now.y, now.z, gap
                );
                let _ = client.set_jumping(false);
                return Ok(true);
            }
            if gap + 0.08 < best_gap || now.y > best_y + 0.08 {
                best_gap = best_gap.min(gap);
                best_y = best_y.max(now.y);
                stale_ticks = 0;
            } else {
                stale_ticks += 1;
            }
            if tick % 10 == 0 {
                println!(
                    "Vertical recovery '{label}': tick={tick}, pos=({:.2}, {:.2}, {:.2}), gap={:.2}, distance={:.2}",
                    now.x, now.y, now.z, gap, distance
                );
            }
            if stale_ticks > 35 && gap > 2.0 {
                break;
            }
            let _ = client.set_direction(yaw_toward(now.x, now.z, step), 0.0);
            self.set_movement_state(client, WalkDirection::Forward, true);
            client.walk(WalkDirection::Forward);
            self.sprint_if_food_reserve(client);
            let _ = client.set_jumping(true);
            client.wait_ticks(1).await;
        }

        client.walk(WalkDirection::None);
        let _ = client.set_jumping(false);
        self.set_movement_state(client, WalkDirection::None, false);
        let end = client
            .get_component::<Position>()
            .map(|p| **p)
            .unwrap_or(start);
        let end_gap = f64::from(step.y) - end.y;
        println!(
            "Vertical recovery '{label}' failed for step {idx}/{total}: end=({:.2}, {:.2}, {:.2}), gap={:.2}",
            end.x, end.y, end.z, end_gap
        );
        Ok(false)
    }

    async fn escape_wall_for_waypoint(
        &self,
        client: &Client,
        pos: BlockPos,
        label: &str,
        left: bool,
    ) -> Result<()> {
        client.force_stop_pathfinding();
        client.walk(WalkDirection::None);
        self.set_movement_state(client, WalkDirection::None, false);
        let _ = client.set_jumping(false);
        client.wait_ticks(2).await;
        let before = client
            .get_component::<Position>()
            .map(|p| **p)
            .ok_or_else(|| anyhow!("bot lost position data before wall recovery"))?;
        let target_yaw = yaw_toward(before.x, before.z, pos);
        println!(
            "Wall recovery '{label}': backing away from obstacle at ({:.2}, {:.2}, {:.2})",
            before.x, before.y, before.z
        );

        for tick in 0..30 {
            if tick % 10 == 0 {
                client.force_stop_pathfinding();
            }
            let _ = client.set_direction(target_yaw, 0.0);
            self.set_movement_state(client, WalkDirection::Backward, false);
            client.walk(WalkDirection::Backward);
            let _ = client.set_jumping(false);
            if tick % 15 == 0 {
                if let Some(now) = client.get_component::<Position>().map(|p| **p) {
                    println!(
                        "Wall recovery '{label}': backing tick={tick}, pos=({:.2}, {:.2}, {:.2})",
                        now.x, now.y, now.z
                    );
                }
            }
            client.wait_ticks(1).await;
        }

        client.walk(WalkDirection::None);
        self.set_movement_state(client, WalkDirection::None, false);
        client.wait_ticks(3).await;

        let side_offset = if left { -65.0 } else { 65.0 };
        let side_yaw = normalize_yaw(target_yaw + side_offset);
        println!(
            "Wall recovery '{label}': diagonal wall-following {} with jump assist",
            if left { "left" } else { "right" }
        );

        for tick in 0..170 {
            if tick % 10 == 0 {
                client.force_stop_pathfinding();
            }
            let _ = client.set_direction(side_yaw, 0.0);
            self.set_movement_state(client, WalkDirection::Forward, true);
            client.walk(WalkDirection::Forward);
            self.sprint_if_food_reserve(client);
            let _ = client.set_jumping(tick % 18 < 6);
            if tick % 25 == 0 {
                if let Some(now) = client.get_component::<Position>().map(|p| **p) {
                    let moved = ((now.x - before.x).powi(2) + (now.z - before.z).powi(2)).sqrt();
                    println!(
                        "Wall recovery '{label}': follow tick={tick}, pos=({:.2}, {:.2}, {:.2}), recovery_moved_xz={:.2}",
                        now.x, now.y, now.z, moved
                    );
                }
            }
            client.wait_ticks(1).await;
        }

        client.walk(WalkDirection::None);
        let _ = client.set_jumping(false);
        self.set_movement_state(client, WalkDirection::None, false);
        client.wait_ticks(3).await;

        if let Some(after) = client.get_component::<Position>().map(|p| **p) {
            let moved = ((after.x - before.x).powi(2) + (after.z - before.z).powi(2)).sqrt();
            println!(
                "Wall recovery '{label}' finished at ({:.2}, {:.2}, {:.2}); moved {:.2} blocks before retrying target",
                after.x, after.y, after.z, moved
            );
        }

        Ok(())
    }

    async fn open_pasadmin_for_step(
        &self,
        client: &Client,
        step: BlockPos,
        push_target: BlockPos,
        label: &str,
        handled_pasadmins: &mut HashSet<(i32, i32, i32)>,
    ) -> bool {
        let candidates = [
            step,
            BlockPos::new(step.x, step.y + 1, step.z),
            BlockPos::new(step.x, step.y - 1, step.z),
        ];
        let mut pasadmin = None;
        {
            let Ok(world) = client.world() else { return false; };
            let world = world.read();
            for pos in candidates {
                let Some(state) = world.get_block_state(pos) else {
                    continue;
                };
                let (id, open) = block_id_and_open(state);
                if is_hand_openable_pasadmin_id(&id) {
                    pasadmin = Some((pos, open == Some(false)));
                    break;
                }
            }
        }
        let Some((pos, should_open)) = pasadmin else {
            return false;
        };
        let key = (pos.x, pos.y, pos.z);
        if handled_pasadmins.contains(&key) {
            return false;
        }
        handled_pasadmins.insert(key);
        client.force_stop_pathfinding();
        client.walk(WalkDirection::None);
        let _ = client.set_jumping(false);
        self.set_movement_state(client, WalkDirection::None, false);
        client.wait_ticks(2).await;
        let approach = client.get_component::<Position>().map(|p| **p);
        if should_open {
            client.look_at(pos.center());
            client.wait_ticks(1).await;
            client.block_interact(pos);
            println!(
                "Walking '{label}': opening door/gate at {}, {}, {} before stepping through",
                pos.x, pos.y, pos.z
            );
            self.publish_status(
                format!(
                    "Walking '{label}': opening door/gate at {}, {}, {}",
                    pos.x, pos.y, pos.z
                ),
                None,
            )
            .await;
            client.wait_ticks(3).await;
        } else {
            println!(
                "Walking '{label}': door/gate at {}, {}, {} is already open; pushing through",
                pos.x, pos.y, pos.z
            );
        }
        let door_center_x = f64::from(pos.x) + 0.5;
        let door_center_z = f64::from(pos.z) + 0.5;
        let mut exit_target = push_target;
        if let Some(approach) = approach {
            let approach_dx = door_center_x - approach.x;
            let approach_dz = door_center_z - approach.z;
            if approach_dx.abs() >= approach_dz.abs() && approach_dx.abs() > 0.2 {
                exit_target = BlockPos::new(pos.x + approach_dx.signum() as i32, pos.y, pos.z);
            } else if approach_dz.abs() > 0.2 {
                exit_target = BlockPos::new(pos.x, pos.y, pos.z + approach_dz.signum() as i32);
            } else {
                let path_dx = push_target.x - step.x;
                let path_dz = push_target.z - step.z;
                if path_dx != 0 || path_dz != 0 {
                    exit_target = BlockPos::new(
                        pos.x + path_dx.signum(),
                        push_target.y,
                        pos.z + path_dz.signum(),
                    );
                }
            }
        }
        let exit_center_x = f64::from(exit_target.x) + 0.5;
        let exit_center_z = f64::from(exit_target.z) + 0.5;
        let clear_target = BlockPos::new(
            exit_target.x + (exit_target.x - pos.x).signum(),
            exit_target.y,
            exit_target.z + (exit_target.z - pos.z).signum(),
        );
        let clear_center_x = f64::from(clear_target.x) + 0.5;
        let clear_center_z = f64::from(clear_target.z) + 0.5;
        self.set_movement_state(client, WalkDirection::Forward, true);
        client.walk(WalkDirection::Forward);
        self.sprint_if_food_reserve(client);
        let _ = client.set_jumping(false);
        for tick in 0..72 {
            if let Some(now) = client.get_component::<Position>().map(|p| **p) {
                let _ = client.set_direction(yaw_toward(now.x, now.z, exit_target), 0.0);
                let exit_distance =
                    ((now.x - exit_center_x).powi(2) + (now.z - exit_center_z).powi(2)).sqrt();
                if exit_distance <= 0.55 {
                    break;
                }
                if tick % 18 == 0 {
                    println!(
                        "Walking '{label}': passing door/gate at {}, {}, {}; pos=({:.2}, {:.2}, {:.2}), exit_distance={:.2}",
                        pos.x, pos.y, pos.z, now.x, now.y, now.z, exit_distance
                    );
                }
            }
            client.wait_ticks(1).await;
        }
        client.walk(WalkDirection::None);
        let _ = client.set_jumping(false);
        self.set_movement_state(client, WalkDirection::None, false);
        client.wait_ticks(3).await;

        let mut safely_past = false;
        for _ in 0..3 {
            safely_past = client
                .get_component::<Position>()
                .map(|p| **p)
                .map(|now| {
                    let bot_block_x = now.x.floor() as i32;
                    let bot_block_z = now.z.floor() as i32;
                    let progress_x = exit_center_x - door_center_x;
                    let progress_z = exit_center_z - door_center_z;
                    let from_door_x = now.x - door_center_x;
                    let from_door_z = now.z - door_center_z;
                    let crossed = from_door_x * progress_x + from_door_z * progress_z;
                    (bot_block_x != pos.x || bot_block_z != pos.z) && crossed > 0.35
                })
                .unwrap_or(false);
            if safely_past {
                break;
            }
            self.set_movement_state(client, WalkDirection::Forward, false);
            client.walk(WalkDirection::Forward);
            let _ = client.set_jumping(false);
            if let Some(now) = client.get_component::<Position>().map(|p| **p) {
                let _ = client.set_direction(yaw_toward(now.x, now.z, exit_target), 0.0);
            }
            client.wait_ticks(8).await;
            client.walk(WalkDirection::None);
            self.set_movement_state(client, WalkDirection::None, false);
            client.wait_ticks(2).await;
        }
        let still_open = safely_past && {
            let Ok(world) = client.world() else { return false; };
            let world = world.read();
            world
                .get_block_state(pos)
                .map(|state| {
                    let (id, open) = block_id_and_open(state);
                    is_hand_openable_pasadmin_id(&id) && open == Some(true)
                })
                .unwrap_or(false)
        };
        if still_open {
            client.walk(WalkDirection::None);
            let _ = client.set_jumping(false);
            self.set_movement_state(client, WalkDirection::None, false);
            client.look_at(pos.center());
            client.wait_ticks(1).await;
            client.block_interact(pos);
            println!(
                "Walking '{label}': closed door/gate at {}, {}, {} after passing through",
                pos.x, pos.y, pos.z
            );
            self.publish_status(
                format!(
                    "Walking '{label}': closed door/gate at {}, {}, {}",
                    pos.x, pos.y, pos.z
                ),
                None,
            )
            .await;
            client.wait_ticks(4).await;
        } else if should_open {
            println!(
                "Walking '{label}': left door/gate at {}, {}, {} open because the bot was not clearly past it yet",
                pos.x, pos.y, pos.z
            );
        }
        self.set_movement_state(client, WalkDirection::Forward, false);
        client.walk(WalkDirection::Forward);
        let _ = client.set_jumping(false);
        for _ in 0..18 {
            if let Some(now) = client.get_component::<Position>().map(|p| **p) {
                let _ = client.set_direction(yaw_toward(now.x, now.z, clear_target), 0.0);
                let clear_distance =
                    ((now.x - clear_center_x).powi(2) + (now.z - clear_center_z).powi(2)).sqrt();
                if clear_distance <= 0.65 {
                    break;
                }
            }
            client.wait_ticks(1).await;
        }
        client.walk(WalkDirection::None);
        let _ = client.set_jumping(false);
        self.set_movement_state(client, WalkDirection::None, false);
        client.wait_ticks(2).await;
        true
    }

    fn set_movement_state(
        &self,
        client: &Client,
        direction: WalkDirection,
        trying_to_sprint: bool,
    ) {
        let _ = client.query_self::<&mut ClientMovementState, _>(|mut physics| {
            physics.move_direction = direction;
            physics.trying_to_sprint = trying_to_sprint;
        });
    }

    fn sprint_if_food_reserve(&self, client: &Client) {
        if client_hunger(client).map(|h| h.food >= 6).unwrap_or(false)
            && edible_inventory_count(client) >= 64
        {
            client.sprint(SprintDirection::Forward);
        } else {
            let _ = client.query_self::<&mut ClientMovementState, _>(|mut physics| {
                physics.trying_to_sprint = false;
            });
        }
    }

    async fn process_chest_ledger(
        &mut self,
        config: ChestLedgerConfig,
    ) -> Result<ChestLedgerResponse> {
        self.halt_if_requested("chest ledger")?;
        let client = self.client_for_name(&config.bot_name)?.clone();
        let pos = BlockPos::new(config.chest_x, config.chest_y, config.chest_z);
        let allowed: HashSet<String> = config
            .allowed_players
            .iter()
            .map(|p| p.trim().to_lowercase())
            .filter(|p| !p.is_empty())
            .collect();
        if allowed.is_empty() {
            return Err(anyhow!("add at least one allowed book writer/signer"));
        }

        self.publish_status(
            format!(
                "Walking to banking chest '{}' at {}, {}, {}",
                config.label, config.chest_x, config.chest_y, config.chest_z
            ),
            None,
        )
        .await;
        self.walk_xz(
            &client,
            pos,
            2.8,
            &format!("banking chest '{}'", config.label),
        )
        .await?;
        client.look_at(pos.center());
        client.wait_ticks(4).await;
        let Ok(Some(chest)) = client.open_container_at(pos).await else {
            return Err(anyhow!(
                "could not open chest at {}, {}, {}",
                config.chest_x,
                config.chest_y,
                config.chest_z
            ));
        };
        client.wait_ticks(5).await;

        let mut processed = Vec::new();
        let mut skipped = Vec::new();
        let contents = chest
            .contents()
            .ok_or_else(|| anyhow!("chest closed before contents could be read"))?;
        for (slot, stack) in contents.iter().enumerate() {
            match read_credits_book(stack, slot, &allowed, &config) {
                Ok(Some((author, recipient, title, amount, page_text))) => {
                    casino::add_credits(&recipient, amount)?;
                    let balance = casino::balance(&recipient)?;
                    processed.push(ChestLedgerEntry {
                        slot,
                        author,
                        recipient,
                        title,
                        amount,
                        page_text,
                        balance,
                    });
                    if config.remove_processed_book {
                        chest.shift_click(slot);
                        client.wait_ticks(2).await;
                    }
                }
                Ok(None) => {}
                Err(reason) => skipped.push(reason),
            }
        }
        drop(chest);
        if config.remove_processed_book && !processed.is_empty() {
            if let (Some(x), Some(y), Some(z)) = (
                config.processed_chest_x,
                config.processed_chest_y,
                config.processed_chest_z,
            ) {
                let moved = self
                    .deposit_written_books_to_processed_chest(&client, BlockPos::new(x, y, z))
                    .await?;
                skipped.push(format!(
                    "banking: moved {moved} verified Credits bill(s) into the trash/archive chest"
                ));
            }
        }
        let mesadmin = format!(
            "Chest ledger processed {} signed book(s), skipped {} item(s).",
            processed.len(),
            skipped.len()
        );
        for entry in &processed {
            println!(
                "banking: credited {} Credits to recipient '{}' from signed book '{}' by '{}' in slot {}. New balance {}.",
                entry.amount,
                entry.recipient,
                entry.title,
                entry.author,
                entry.slot,
                entry.balance.credits
            );
        }
        for reason in &skipped {
            println!("banking skipped: {reason}");
        }
        self.publish_status(mesadmin.clone(), None).await;
        Ok(ChestLedgerResponse {
            mesadmin,
            processed,
            skipped,
        })
    }

    async fn write_sign_place_book(
        &mut self,
        request: BookWriterRequest,
    ) -> Result<BookWriterResponse> {
        let client = self.client_for_name(&request.bot_name)?.clone();
        let open_book_events = self
            .connected_bot_for_name(&request.bot_name)?
            .open_book_events
            .clone();
        let title = request.title.trim().to_string();
        if title.is_empty() {
            return Err(anyhow!("book title is required to sign the book"));
        }
        if title.chars().count() > 32 {
            return Err(anyhow!("book title must be 32 characters or fewer"));
        }
        let mut pages: Vec<String> = request
            .pages
            .iter()
            .map(|page| page.trim_end().to_string())
            .filter(|page| !page.trim().is_empty())
            .collect();
        let recipient = request.recipient.trim();
        if !recipient.is_empty() {
            if pages.is_empty() {
                pages.push(format!("Credits Deposit
Recipient: {recipient}"));
            } else {
                let first_page_lines: Vec<&str> = pages[0].lines().collect();
                if first_page_lines
                    .iter()
                    .map(|line| line.trim())
                    .filter(|line| !line.is_empty())
                    .count()
                    < 2
                {
                    pages[0] = format!("{}
Recipient: {recipient}", pages[0].trim_end());
                }
            }
        }
        if pages.is_empty() {
            return Err(anyhow!("add at least one non-empty book page"));
        }
        if pages.len() > 100 {
            return Err(anyhow!("a signed book can have at most 100 pages"));
        }
        for (idx, page) in pages.iter().enumerate() {
            if page.chars().count() > 1024 {
                return Err(anyhow!(
                    "page {} is too long; keep each page at 1024 characters or fewer",
                    idx + 1
                ));
            }
        }

        let requested_slot = request.inventory_slot.map(|slot| slot.min(35));
        let inventory_slot = prepare_writable_book_hotbar(&client, requested_slot).await?;
        let found_mesadmin =
            format!("Book writer found writable book in hotbar slot {inventory_slot}");
        println!("{found_mesadmin}");
        self.publish_status(found_mesadmin, None).await;
        log_book_stack(&client, inventory_slot, "after staging writable book");

        let edit_slots = book_edit_slot_candidates(inventory_slot);
        let write_mesadmin = format!(
            "Writing/signing {} page(s) into book at hotbar slot {} using edit slot(s) {:?}",
            pages.len(),
            inventory_slot,
            edit_slots
        );
        println!("{write_mesadmin}");
        self.publish_status(write_mesadmin, None).await;
        let sign_mesadmin = format!("Signing book '{}' at hotbar slot {}", title, inventory_slot);
        println!("{sign_mesadmin}");
        self.publish_status(sign_mesadmin, None).await;
        let mut signed_book_seen = false;
        for attempt in 1..=3 {
            let previous_open_count = *open_book_events.read().await;
            println!(
                "Book writer vanilla signing attempt {attempt} for '{title}', selected_hotbar={}, previous_open_book_count={previous_open_count}",
                client.selected_hotbar_slot().unwrap_or(0)
            );
            client.start_use_item();
            let open_confirmed =
                wait_for_open_book_event(&client, &open_book_events, previous_open_count, 18).await;
            println!(
                "Book writer attempt {attempt}: open_book_confirmed={open_confirmed}, open_book_count={}",
                *open_book_events.read().await
            );
            log_book_stack(&client, inventory_slot, "before signed packet");

            for slot in edit_slots_for_attempt(&edit_slots, attempt) {
                println!("Book writer signed packet attempt {attempt} using edit slot {slot}");
                client.write_packet(ServerboundEditBook {
                    slot,
                    pages: pages.clone(),
                    title: Some(title.clone()),
                });
                client.wait_ticks(4).await;
                log_book_stack(&client, inventory_slot, "after signed packet");
                if wait_for_written_book(&client, inventory_slot, &title).await {
                    signed_book_seen = true;
                    break;
                }
            }
            if signed_book_seen {
                break;
            }
            if !matches!(
                inventory_stack_for_slot(&client, inventory_slot).map(|stack| stack.kind()),
                Some(ItemKind::WritableBook)
            ) {
                break;
            }
            client.wait_ticks(10).await;
        }
        if !signed_book_seen {
            return Err(anyhow!(
                "server did not confirm the signed book in hotbar slot {inventory_slot} after trying edit slot(s) {:?}; not moving an empty book into the chest",
                edit_slots
            ));
        }
        let signed_mesadmin = format!(
            "Book writer confirmed signed book '{}' in hotbar slot {}",
            title, inventory_slot
        );
        println!("{signed_mesadmin}");
        self.publish_status(signed_mesadmin, None).await;
        client.wait_ticks(4).await;

        let mut placed_in_chest = false;
        if request.place_in_chest {
            let pos = BlockPos::new(request.chest_x, request.chest_y, request.chest_z);
            self.publish_status(
                format!(
                    "Walking to book deposit chest at {}, {}, {}",
                    request.chest_x, request.chest_y, request.chest_z
                ),
                None,
            )
            .await;
            self.walk_xz(&client, pos, 2.8, "book deposit chest")
                .await?;
            client.look_at(pos.center());
            client.wait_ticks(4).await;
            let Ok(Some(chest)) = client.open_container_at(pos).await else {
                return Err(anyhow!(
                    "signed the book, but could not open chest at {}, {}, {}",
                    request.chest_x,
                    request.chest_y,
                    request.chest_z
                ));
            };
            client.wait_ticks(5).await;
            let menu = chest.menu()?.ok_or_else(|| {
                anyhow!("chest closed before the signed book could be moved into it")
            })?;
            let protocol_slot = player_inventory_protocol_slot(&menu, inventory_slot);
            chest.shift_click(protocol_slot);
            client.wait_ticks(5).await;
            placed_in_chest = true;
            println!("Book writer moved signed book '{}' into chest", title);
        }

        let bot_name = if request.bot_name.trim().is_empty() {
            "first connected bot".to_string()
        } else {
            clean_bot_name(&request.bot_name)
        };
        let mesadmin = if placed_in_chest {
            format!(
                "Signed '{}' with {} page(s) and moved it into the chest.",
                title,
                pages.len()
            )
        } else {
            format!(
                "Signed '{}' with {} page(s). It remains in inventory slot {}.",
                title,
                pages.len(),
                inventory_slot
            )
        };
        self.publish_status(mesadmin.clone(), None).await;
        Ok(BookWriterResponse {
            mesadmin,
            bot_name,
            title,
            page_count: pages.len(),
            inventory_slot,
            placed_in_chest,
        })
    }

    async fn butler_transfer(
        &mut self,
        request: ButlerTransferRequest,
    ) -> Result<ButlerTransferResponse> {
        self.halt_if_requested("bank butler")?;
        let source = storage::find_butler_chest(request.source_chest_id)
            .ok_or_else(|| anyhow!("source chest waypoint not found"))?;
        let destination = storage::find_butler_waypoint(request.destination_waypoint_id)
            .ok_or_else(|| anyhow!("destination waypoint not found"))?;
        let shulker_name = request.shulker_name.trim().to_string();
        if shulker_name.is_empty() {
            return Err(anyhow!("renamed shulker name is required"));
        }
        let bot_key = if !source.bot_name.trim().is_empty() {
            source.bot_name.clone()
        } else {
            destination.bot_name.clone()
        };
        let client = self.client_for_name(&bot_key)?.clone();
        let bot_name = if bot_key.trim().is_empty() {
            "first connected bot".to_string()
        } else {
            clean_bot_name(&bot_key)
        };

        self.take_named_shulker_from_butler_chest(&client, &source, &shulker_name)
            .await?;
        let inventory_slot =
            find_named_shulker_inventory_slot(&client, &shulker_name).ok_or_else(|| {
                anyhow!("picked up '{shulker_name}', but could not find it in inventory")
            })?;
        self.deposit_named_shulker_to_butler_waypoint(
            &client,
            &destination,
            &shulker_name,
            inventory_slot,
        )
        .await?;

        let mesadmin = format!(
            "Bank Butler moved renamed shulker '{}' from '{}' to '{}'.",
            shulker_name, source.label, destination.label
        );
        self.publish_status(mesadmin.clone(), None).await;
        Ok(ButlerTransferResponse {
            mesadmin,
            bot_name,
            shulker_name,
            source_label: source.label,
            destination_label: destination.label,
        })
    }

    async fn take_named_shulker_from_butler_chest(
        &mut self,
        client: &Client,
        source: &ButlerChestEntry,
        shulker_name: &str,
    ) -> Result<()> {
        let pos = BlockPos::new(source.chest_x, source.chest_y, source.chest_z);
        self.publish_status(
            format!(
                "Bank Butler walking to source chest '{}' at {}, {}, {}",
                source.label, source.chest_x, source.chest_y, source.chest_z
            ),
            None,
        )
        .await;
        self.walk_xz(
            client,
            pos,
            2.8,
            &format!("butler source chest '{}'", source.label),
        )
        .await?;
        client.look_at(pos.center());
        client.wait_ticks(4).await;
        let Ok(Some(chest)) = client.open_container_at(pos).await else {
            return Err(anyhow!(
                "could not open Bank Butler source chest '{}' at {}, {}, {}",
                source.label,
                source.chest_x,
                source.chest_y,
                source.chest_z
            ));
        };
        client.wait_ticks(5).await;
        let contents = chest
            .contents()
            .ok_or_else(|| anyhow!("source chest closed before contents could be read"))?;
        let Some(slot) = contents
            .iter()
            .enumerate()
            .find_map(|(slot, stack)| is_named_shulker(stack, shulker_name).then_some(slot))
        else {
            return Err(anyhow!(
                "source chest '{}' did not contain renamed shulker '{}' (case-sensitive)",
                source.label,
                shulker_name
            ));
        };
        println!(
            "Bank Butler found renamed shulker '{shulker_name}' in source chest '{}' slot {slot}",
            source.label
        );
        chest.shift_click(slot);
        client.wait_ticks(6).await;
        drop(chest);
        Ok(())
    }

    async fn deposit_named_shulker_to_butler_waypoint(
        &mut self,
        client: &Client,
        destination: &ButlerWaypointEntry,
        shulker_name: &str,
        inventory_slot: u8,
    ) -> Result<()> {
        let pos = BlockPos::new(
            destination.chest_x,
            destination.chest_y,
            destination.chest_z,
        );
        self.publish_status(
            format!(
                "Bank Butler walking to destination chest '{}' at {}, {}, {}",
                destination.label, destination.chest_x, destination.chest_y, destination.chest_z
            ),
            None,
        )
        .await;
        self.walk_xz(
            client,
            pos,
            2.8,
            &format!("butler destination chest '{}'", destination.label),
        )
        .await?;
        client.look_at(pos.center());
        client.wait_ticks(4).await;
        let Ok(Some(chest)) = client.open_container_at(pos).await else {
            return Err(anyhow!(
                "could not open Bank Butler destination chest '{}' at {}, {}, {}",
                destination.label,
                destination.chest_x,
                destination.chest_y,
                destination.chest_z
            ));
        };
        client.wait_ticks(5).await;
        let menu = chest
            .menu()?
            .ok_or_else(|| anyhow!("destination chest closed before shulker could be deposited"))?;
        let protocol_slot = player_inventory_protocol_slot(&menu, inventory_slot);
        chest.shift_click(protocol_slot);
        client.wait_ticks(6).await;
        if find_named_shulker_inventory_slot(client, shulker_name).is_some() {
            return Err(anyhow!(
                "tried to deposit renamed shulker '{shulker_name}', but it still appears in inventory"
            ));
        }
        println!(
            "Bank Butler deposited renamed shulker '{shulker_name}' into destination chest '{}'",
            destination.label
        );
        Ok(())
    }

    async fn deposit_written_books_to_processed_chest(
        &mut self,
        client: &Client,
        pos: BlockPos,
    ) -> Result<usize> {
        self.publish_status(
            format!(
                "Walking to trash/archive chest at {}, {}, {}",
                pos.x, pos.y, pos.z
            ),
            None,
        )
        .await;
        self.walk_xz(client, pos, 2.8, "trash/archive chest")
            .await?;
        client.look_at(pos.center());
        client.wait_ticks(4).await;
        let Ok(Some(chest)) = client.open_container_at(pos).await else {
            return Err(anyhow!(
                "credited the Credits bills, but could not open trash/archive chest at {}, {}, {}",
                pos.x,
                pos.y,
                pos.z
            ));
        };
        client.wait_ticks(5).await;
        let menu = chest
            .menu()?
            .ok_or_else(|| anyhow!("trash/archive chest closed before books could be moved"))?;
        let mut moved = 0;
        for inventory_slot in 0..=35_u8 {
            if matches!(
                inventory_stack_for_slot(client, inventory_slot).map(|stack| stack.kind()),
                Some(ItemKind::WrittenBook)
            ) {
                let protocol_slot = player_inventory_protocol_slot(&menu, inventory_slot);
                chest.shift_click(protocol_slot);
                client.wait_ticks(2).await;
                moved += 1;
            }
        }
        Ok(moved)
    }

    async fn viewport_snapshot(
        &mut self,
        request: ViewportSnapshotRequest,
    ) -> Result<ViewportSnapshot> {
        let client = self.client_for_name(&request.bot_name)?.clone();
        let world_time = self
            .connected_bot_for_name(&request.bot_name)?
            .world_time
            .clone();
        let pos = client
            .get_component::<Position>()
            .map(|p| **p)
            .ok_or_else(|| anyhow!("bot is connected but has not received position data yet"))?;
        let chunks = request.chunks.clamp(1, 32);
        let center_x = pos.x.floor() as i32;
        let center_y = pos.y.floor() as i32;
        let center_z = pos.z.floor() as i32;
        let size = i32::from(chunks) * 16;
        let half_width = size / 2;
        let start_x = center_x - half_width;
        let start_z = center_z - half_width;
        let end_x = start_x + size - 1;
        let end_z = start_z + size - 1;
        let y_top = center_y + 24;
        let y_bottom = center_y - 32;
        let world = client.world().map_err(|e| anyhow!("world component unavailable: {e}"))?;
        let world = world.read();
        let _biome_id = world.get_biome(BlockPos::new(center_x, center_y, center_z));
        let mut blocks = Vec::new();
        let mut scanned_columns = 0usize;

        for x in start_x..=end_x {
            for z in start_z..=end_z {
                scanned_columns += 1;
                for y in (y_bottom..=y_top).rev() {
                    let block_pos = BlockPos::new(x, y, z);
                    let Some(state) = world.get_block_state(block_pos) else {
                        continue;
                    };
                    if state.is_air() {
                        continue;
                    }
                    let kind = format!("{:?}", azalea_registry::builtin::BlockKind::from(state));
                    blocks.push(ViewportBlock { x, y, z, kind });
                    break;
                }
            }
        }
        let live_blocks = blocks.len();
        if let Err(err) = storage::merge_viewport_cache(&blocks) {
            eprintln!("viewport cache save failed: {err}");
        }
        let mut by_column: HashMap<(i32, i32), ViewportBlock> =
            storage::cached_viewport_blocks(start_x, end_x, start_z, end_z)
                .into_iter()
                .map(|block| ((block.x, block.z), block))
                .collect();
        for block in blocks {
            by_column.insert((block.x, block.z), block);
        }
        let mut blocks: Vec<_> = by_column.into_values().collect();
        blocks.sort_by_key(|block| (block.x, block.z));
        println!(
            "Viewport snapshot for '{}': {} live column(s), {} cached/known column(s) in {} chunk square",
            request.bot_name,
            live_blocks,
            blocks.len(),
            chunks
        );
        drop(world);
        let biome = None;
        let time = world_time.read().await.clone().map(estimate_world_time);

        Ok(ViewportSnapshot {
            bot_name: request.bot_name,
            center_x: pos.x,
            center_y: pos.y,
            center_z: pos.z,
            biome,
            day_time: time.as_ref().map(|t| t.day_time as i64),
            time_of_day: time.as_ref().map(|t| t.time_of_day as i64),
            time_label: time
                .as_ref()
                .map(|t| t.time_label.clone())
                .unwrap_or_else(|| "Awaiting Minecraft time packet".to_string()),
            chunks,
            scanned_columns,
            blocks,
        })
    }
}

fn read_credits_book(
    stack: &ItemStack,
    slot: usize,
    allowed: &HashSet<String>,
    config: &ChestLedgerConfig,
) -> std::result::Result<Option<(String, String, String, i64, String)>, String> {
    if stack.kind() == ItemKind::Air {
        return Ok(None);
    }
    if stack.kind() != ItemKind::WrittenBook {
        return Ok(None);
    }
    let Some(content) = stack.get_component::<WrittenBookContent>() else {
        return Err(format!(
            "slot {slot}: written book had no signed-book content"
        ));
    };
    let author = content.author.trim().to_string();
    if !allowed.is_empty() && !allowed.contains(&author.to_lowercase()) {
        return Err(format!(
            "slot {slot}: writer/signer '{author}' is not on the allowed list"
        ));
    }
    let title = content.title.raw.to_string();
    let page_text = content
        .pages
        .iter()
        .map(|page| page.raw.to_string())
        .collect::<Vec<_>>()
        .join("
");
    let recipient = parse_book_recipient(&page_text)
        .ok_or_else(|| format!("slot {slot}: no recipient found on line 2 of the book"))?;
    let amount = parse_credits_amount(&page_text)
        .or_else(|| parse_credits_amount(&title))
        .ok_or_else(|| format!("slot {slot}: no Credits amount found in book"))?;
    if amount < config.min_credits || amount > config.max_credits {
        return Err(format!(
            "slot {slot}: amount {amount} outside allowed range {}-{}",
            config.min_credits, config.max_credits
        ));
    }
    Ok(Some((author, recipient, title, amount, page_text)))
}

fn parse_book_recipient(text: &str) -> Option<String> {
    let line = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .nth(1)?;
    let value = line
        .split_once(':')
        .map(|(_, value)| value.trim())
        .unwrap_or(line)
        .trim_matches(|ch: char| ch == '-' || ch == '=' || ch.is_whitespace())
        .trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_credits_amount(text: &str) -> Option<i64> {
    for line in text.lines() {
        let lower = line.to_lowercase();
        if lower.contains("credits") || lower.contains("amount") || lower.contains("value") {
            if let Some(amount) = first_integer(line) {
                return Some(amount);
            }
        }
    }
    first_integer(text)
}

fn first_integer(text: &str) -> Option<i64> {
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() || (ch == '-' && current.is_empty()) {
            current.push(ch);
        } else if !current.is_empty() && current != "-" {
            return current.parse().ok();
        } else {
            current.clear();
        }
    }
    if current.is_empty() || current == "-" {
        None
    } else {
        current.parse().ok()
    }
}

fn clean_bot_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.eq_ignore_ascii_case("main") || trimmed.eq_ignore_ascii_case("iosbot") {
        "UtilityBot".to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_edible_stack(stack: &ItemStack) -> bool {
    stack.get_component::<Food>().is_some() && stack.get_component::<Consumable>().is_some()
}

fn client_hunger(client: &Client) -> Option<Hunger> {
    client
        .get_component::<Hunger>()
        .map(|hunger| hunger.clone())
}

fn edible_inventory_count(client: &Client) -> i32 {
    (0..=35_u8)
        .filter_map(|slot| inventory_stack_for_slot(client, slot))
        .filter(|stack| is_edible_stack(stack))
        .filter_map(|stack| stack.as_present().map(|data| data.count.max(0)))
        .sum()
}

fn find_best_edible_inventory_slot(client: &Client, hotbar_only: bool) -> Option<(u8, i32)> {
    let max_slot = if hotbar_only { 8 } else { 35 };
    (0..=max_slot)
        .filter_map(|slot| {
            let stack = inventory_stack_for_slot(client, slot)?;
            let food = stack.get_component::<Food>()?;
            let count = stack
                .as_present()
                .map(|data| data.count.max(0))
                .unwrap_or(0);
            if count <= 0 || stack.get_component::<Consumable>().is_none() {
                return None;
            }
            Some((slot, food.nutrition, count))
        })
        .max_by_key(|(_, nutrition, count)| (*nutrition, *count))
        .map(|(slot, nutrition, _)| (slot, nutrition))
}

async fn auto_eat_if_hungry(client: &Client, label: &str, allow_inventory_swap: bool) -> bool {
    let Some(hunger) = client_hunger(client) else {
        return false;
    };
    if hunger.food >= 18 {
        return false;
    }
    let total_food = edible_inventory_count(client);
    if total_food <= 32 {
        if hunger.food <= 10 {
            println!(
                "Auto-eat '{label}': hunger is {}, but only {total_food} edible item(s) remain; preserving the 32-food reserve",
                hunger.food
            );
        }
        return false;
    }
    let Some((source_slot, nutrition)) =
        find_best_edible_inventory_slot(client, !allow_inventory_swap)
            .or_else(|| find_best_edible_inventory_slot(client, false))
    else {
        return false;
    };
    if source_slot > 8 && !allow_inventory_swap {
        return false;
    }

    client.walk(WalkDirection::None);
    let _ = client.set_jumping(false);
    let _ = client.query_self::<&mut ClientMovementState, _>(|mut physics| {
        physics.move_direction = WalkDirection::None;
        physics.trying_to_sprint = false;
    });
    let previous_hotbar = client.selected_hotbar_slot().unwrap_or(0).min(8);
    let mut food_hotbar = source_slot;
    if source_slot > 8 {
        let target_hotbar_slot = find_empty_hotbar_slot(client).unwrap_or(previous_hotbar);
        let Ok(Some(inventory)) = client.open_inventory() else {
            println!("Auto-eat '{label}': could not open inventory to move food");
            return false;
        };
        let Ok(Some(menu)) = inventory.menu() else {
            println!("Auto-eat '{label}': inventory menu unavailable while moving food");
            return false;
        };
        let source_protocol_slot = player_inventory_protocol_slot(&menu, source_slot);
        println!(
            "Auto-eat '{label}': moving food from inventory slot {source_slot} to hotbar slot {target_hotbar_slot}"
        );
        inventory.click(SwapClick {
            source_slot: source_protocol_slot as u16,
            target_slot: target_hotbar_slot,
        });
        client.wait_ticks(8).await;
        drop(inventory);
        client.wait_ticks(3).await;
        food_hotbar = target_hotbar_slot;
    }

    client.set_selected_hotbar_slot(food_hotbar);
    client.wait_ticks(3).await;
    println!(
        "Auto-eat '{label}': eating from hotbar slot {food_hotbar}, hunger={}, food reserve={}, nutrition={nutrition}",
        hunger.food, total_food
    );
    client.start_use_item();
    client.wait_ticks(45).await;
    if previous_hotbar != food_hotbar {
        client.set_selected_hotbar_slot(previous_hotbar);
        client.wait_ticks(2).await;
    }
    true
}

fn player_inventory_protocol_slot(menu: &Menu, inventory_slot: u8) -> usize {
    let slot = inventory_slot.min(35) as usize;
    if slot <= 8 {
        *menu.hotbar_slots_range().start() + slot
    } else {
        *menu.player_slots_without_hotbar_range().start() + (slot - 9)
    }
}

fn inventory_stack_for_slot(client: &Client, inventory_slot: u8) -> Option<ItemStack> {
    let inventory = client.get_inventory().ok()?;
    let menu = inventory.menu().ok()??;
    let protocol_slot = player_inventory_protocol_slot(&menu, inventory_slot);
    inventory.slots()?.get(protocol_slot).cloned()
}

fn find_named_shulker_inventory_slot(client: &Client, shulker_name: &str) -> Option<u8> {
    (0..=35).find(|slot| {
        inventory_stack_for_slot(client, *slot)
            .map(|stack| is_named_shulker(&stack, shulker_name))
            .unwrap_or(false)
    })
}

fn is_named_shulker(stack: &ItemStack, shulker_name: &str) -> bool {
    is_shulker_box_item(stack.kind())
        && stack
            .get_component::<CustomName>()
            .map(|name| name.name.to_string() == shulker_name)
            .unwrap_or(false)
}

fn is_shulker_box_item(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::ShulkerBox
            | ItemKind::WhiteShulkerBox
            | ItemKind::OrangeShulkerBox
            | ItemKind::MagentaShulkerBox
            | ItemKind::LightBlueShulkerBox
            | ItemKind::YellowShulkerBox
            | ItemKind::LimeShulkerBox
            | ItemKind::PinkShulkerBox
            | ItemKind::GrayShulkerBox
            | ItemKind::LightGrayShulkerBox
            | ItemKind::CyanShulkerBox
            | ItemKind::PurpleShulkerBox
            | ItemKind::BlueShulkerBox
            | ItemKind::BrownShulkerBox
            | ItemKind::GreenShulkerBox
            | ItemKind::RedShulkerBox
            | ItemKind::BlackShulkerBox
    )
}

fn find_writable_book_slot(client: &Client) -> Option<u8> {
    (0..=35).find(|slot| {
        inventory_stack_for_slot(client, *slot)
            .map(|stack| stack.kind() == ItemKind::WritableBook)
            .unwrap_or(false)
    })
}

fn book_edit_slot_candidates(hotbar_slot: u8) -> Vec<u32> {
    let hotbar_slot = hotbar_slot.min(8);
    vec![u32::from(hotbar_slot), u32::from(36 + hotbar_slot)]
}

fn edit_slots_for_attempt(edit_slots: &[u32], attempt: usize) -> Vec<u32> {
    match attempt {
        1 => edit_slots.first().copied().into_iter().collect(),
        2 => edit_slots.iter().copied().take(1).collect(),
        _ => edit_slots.to_vec(),
    }
}

async fn wait_for_open_book_event(
    client: &Client,
    open_book_events: &Arc<RwLock<u64>>,
    previous_count: u64,
    ticks: u32,
) -> bool {
    for _ in 0..ticks {
        client.wait_ticks(1).await;
        if *open_book_events.read().await > previous_count {
            return true;
        }
    }
    false
}

fn describe_book_stack(stack: &ItemStack) -> String {
    let writable_pages = stack
        .get_component::<WritableBookContent>()
        .map(|content| content.pages.len());
    let written = stack.get_component::<WrittenBookContent>().map(|content| {
        format!(
            "title='{}', author='{}', pages={}",
            content.title.raw,
            content.author,
            content.pages.len()
        )
    });
    format!(
        "kind={:?}, count={}, writable_pages={:?}, written={:?}",
        stack.kind(),
        stack.count(),
        writable_pages,
        written
    )
}

fn log_book_stack(client: &Client, hotbar_slot: u8, label: &str) {
    let selected = client.selected_hotbar_slot().unwrap_or(0);
    let held = client.get_held_item().ok();
    let slot = inventory_stack_for_slot(client, hotbar_slot);
    println!(
        "Book writer stack {label}: selected_hotbar={selected}, held={}, slot{}={}",
        held.as_ref().map(describe_book_stack).unwrap_or_else(|| "unreadable".to_string()),
        hotbar_slot,
        slot.as_ref()
            .map(describe_book_stack)
            .unwrap_or_else(|| "unreadable".to_string())
    );
}

fn find_empty_hotbar_slot(client: &Client) -> Option<u8> {
    (0..=8).find(|slot| {
        inventory_stack_for_slot(client, *slot)
            .map(|stack| stack.is_empty())
            .unwrap_or(false)
    })
}

async fn prepare_writable_book_hotbar(client: &Client, requested_slot: Option<u8>) -> Result<u8> {
    let source_slot = if let Some(slot) = requested_slot {
        let stack = inventory_stack_for_slot(client, slot).ok_or_else(|| {
            anyhow!(
                "could not read bot inventory slot {slot}; close any open container and try again"
            )
        })?;
        if stack.kind() != ItemKind::WritableBook {
            return Err(anyhow!(
                "inventory slot {slot} must contain a writable book/book and quill, but found {:?}",
                stack.kind()
            ));
        }
        slot
    } else {
        find_writable_book_slot(client).ok_or_else(|| {
            anyhow!("could not find a writable book/book and quill anywhere in the bot inventory")
        })?
    };

    if source_slot <= 8 {
        client.set_selected_hotbar_slot(source_slot);
        client.wait_ticks(3).await;
        println!("Book writer selected writable book in hotbar slot {source_slot}");
        return Ok(source_slot);
    }

    let target_hotbar_slot =
        find_empty_hotbar_slot(client).unwrap_or_else(|| client.selected_hotbar_slot().unwrap_or(0).min(8));
    let inventory = client.open_inventory()?.ok_or_else(|| {
        anyhow!("could not open the bot inventory to move the writable book into the hotbar")
    })?;
    let menu = inventory.menu()?.ok_or_else(|| {
        anyhow!("inventory menu was not available while moving the writable book into the hotbar")
    })?;
    let source_protocol_slot = player_inventory_protocol_slot(&menu, source_slot);
    println!(
        "Book writer swapping writable book from inventory slot {source_slot} into hotbar slot {target_hotbar_slot}"
    );
    inventory.click(SwapClick {
        source_slot: source_protocol_slot as u16,
        target_slot: target_hotbar_slot,
    });
    client.wait_ticks(8).await;
    drop(inventory);
    client.wait_ticks(3).await;

    let stack = inventory_stack_for_slot(client, target_hotbar_slot).ok_or_else(|| {
        anyhow!(
            "could not confirm writable book after moving it to hotbar slot {target_hotbar_slot}"
        )
    })?;
    if stack.kind() != ItemKind::WritableBook {
        return Err(anyhow!(
            "hotbar slot {target_hotbar_slot} did not contain the writable book after moving; found {:?}",
            stack.kind()
        ));
    }
    client.set_selected_hotbar_slot(target_hotbar_slot);
    client.wait_ticks(3).await;
    Ok(target_hotbar_slot)
}

async fn wait_for_written_book(client: &Client, hotbar_slot: u8, expected_title: &str) -> bool {
    for _ in 0..30 {
        client.wait_ticks(1).await;
        if let Some(stack) = inventory_stack_for_slot(client, hotbar_slot) {
            if stack.kind() == ItemKind::WrittenBook {
                if let Some(content) = stack.get_component::<WrittenBookContent>() {
                    return content.title.raw == expected_title;
                }
                return true;
            }
        }
    }
    false
}

fn minecraft_time_label(time_of_day: u64) -> String {
    let day_tick = time_of_day % 24_000;
    let hour = ((day_tick / 1_000) + 6) % 24;
    let minute = ((day_tick % 1_000) * 60) / 1_000;
    let phase = match day_tick {
        0..=999 => "Sunrise",
        1_000..=5_999 => "Morning",
        6_000..=11_999 => "Day",
        12_000..=12_999 => "Sunset",
        13_000..=17_999 => "Night",
        18_000..=22_999 => "Midnight",
        _ => "Dawn",
    };
    format!("{phase} {hour:02}:{minute:02} ({day_tick} ticks)")
}

fn estimate_world_time(snapshot: WorldTimeSnapshot) -> WorldTimeSnapshot {
    let elapsed_ticks = (snapshot.received_at.elapsed().as_millis() as u64 * 20) / 1000;
    let day_time = snapshot.day_time.saturating_add(elapsed_ticks);
    let time_of_day = day_time % 24_000;
    WorldTimeSnapshot {
        day_time,
        time_of_day,
        time_label: minecraft_time_label(time_of_day),
        received_at: snapshot.received_at,
    }
}

#[allow(dead_code)]
fn humanize_identifier(identifier: &str) -> String {
    let raw = identifier
        .rsplit(':')
        .next()
        .unwrap_or(identifier)
        .replace('_', " ");
    raw.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut out = first.to_uppercase().to_string();
                    out.push_str(chars.as_str());
                    out
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn yaw_toward(current_x: f64, current_z: f64, target: BlockPos) -> f32 {
    let target_x = f64::from(target.x) + 0.5;
    let target_z = f64::from(target.z) + 0.5;
    yaw_between(current_x, current_z, target_x, target_z)
}

fn yaw_between(current_x: f64, current_z: f64, target_x: f64, target_z: f64) -> f32 {
    let delta_x = target_x - current_x;
    let delta_z = target_z - current_z;
    normalize_yaw(f64::atan2(-delta_x, delta_z).to_degrees() as f32)
}

fn avoid_zone_tangent_yaw(
    current_x: f64,
    current_z: f64,
    target: BlockPos,
    avoid_zones: &[(f64, f64, i32)],
) -> Option<f32> {
    let target_x = f64::from(target.x) + 0.5;
    let target_z = f64::from(target.z) + 0.5;
    let target_dx = target_x - current_x;
    let target_dz = target_z - current_z;
    let target_len = (target_dx * target_dx + target_dz * target_dz)
        .sqrt()
        .max(0.001);
    let target_dir = (target_dx / target_len, target_dz / target_len);

    let mut selected = None;
    let mut selected_distance = f64::MAX;
    for (anchor_x, anchor_z, _) in avoid_zones {
        let away_x = current_x - *anchor_x;
        let away_z = current_z - *anchor_z;
        let distance = (away_x * away_x + away_z * away_z).sqrt();
        if !(4.0..=34.0).contains(&distance) {
            continue;
        }
        let away = (away_x / distance, away_z / distance);
        let moving_toward_anchor = target_dir.0 * away.0 + target_dir.1 * away.1 < 0.25;
        if !moving_toward_anchor {
            continue;
        }
        if distance < selected_distance {
            selected_distance = distance;
            let tangent_a = (-away.1, away.0);
            let tangent_b = (away.1, -away.0);
            let score_a = tangent_a.0 * target_dir.0 + tangent_a.1 * target_dir.1;
            let score_b = tangent_b.0 * target_dir.0 + tangent_b.1 * target_dir.1;
            let tangent = if score_a >= score_b {
                tangent_a
            } else {
                tangent_b
            };
            selected = Some(yaw_between(
                current_x,
                current_z,
                current_x + tangent.0,
                current_z + tangent.1,
            ));
        }
    }
    selected
}

fn normalize_yaw(mut yaw: f32) -> f32 {
    if yaw > 180.0 {
        yaw -= 360.0;
    } else if yaw < -180.0 {
        yaw += 360.0;
    }
    yaw
}

fn block_id_and_open(state: BlockState) -> (String, Option<bool>) {
    let block: Box<dyn BlockTrait> = Box::<dyn BlockTrait>::from(state);
    let id = block.id().to_string();
    let open = block.get_property("open").map(|value| value == "true");
    (id, open)
}

fn is_hand_openable_pasadmin_id(id: &str) -> bool {
    if id == "iron_door" {
        return false;
    }
    id.ends_with("_door") || id.ends_with("_fence_gate")
}

fn is_openable_pasadmin(state: BlockState) -> bool {
    let (id, _) = block_id_and_open(state);
    is_hand_openable_pasadmin_id(&id)
}

fn is_walkable_body_block(state: BlockState) -> bool {
    state.is_air() || is_openable_pasadmin(state)
}

fn is_walkable_head_block(state: BlockState) -> bool {
    state.is_air() || is_openable_pasadmin(state)
}

fn minecraft_user_has_permission(player: &str, permission: &str) -> bool {
    let player_l = player.trim().to_lowercase();
    storage::load_users().into_iter().any(|u| {
        let user_match = u.username.eq_ignore_ascii_case(&player_l)
            || u.minecraft_name.eq_ignore_ascii_case(&player_l);
        user_match && (matches!(u.role, crate::models::UserRole::Owner)
            || u.permissions.iter().any(|p| p.eq_ignore_ascii_case(permission) || p.eq_ignore_ascii_case("faq_admin")))
    })
}


fn parse_faq_query_command(line: &str) -> Option<(String, String)> {
    let lower = line.to_lowercase();
    let command_start = lower.find("?faq")?;
    let command_tail = &lower[command_start..];
    if command_tail.starts_with("?faqadd") || command_tail.starts_with("?addfaq")
        || command_tail.starts_with("?faqset") || command_tail.starts_with("?faqdel")
        || command_tail.starts_with("?faqdelete")
    {
        return None;
    }
    let sender = infer_chat_sender(&line[..command_start]).unwrap_or_else(|| "Player".to_string());
    let command = line[command_start..].trim();
    let query = command.split_once(' ').map(|(_, rest)| rest.trim()).unwrap_or("list").to_string();
    Some((sender, if query.is_empty() { "list".to_string() } else { query }))
}

fn enabled_faq_rows() -> Vec<serde_json::Value> {
    storage::public_load("faqs").unwrap_or_default()
        .into_iter()
        .filter(|r| r.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true))
        .collect()
}

fn faq_text(row: &serde_json::Value) -> String {
    if let Some(text) = row.get("text").and_then(|v| v.as_str()) {
        return text.trim().to_string();
    }
    let question = row.get("question").and_then(|v| v.as_str()).unwrap_or("").trim();
    let answer = row.get("answer").and_then(|v| v.as_str()).unwrap_or("").trim();
    match (question.is_empty(), answer.is_empty()) {
        (false, false) => format!("{question} - {answer}"),
        (false, true) => question.to_string(),
        (true, false) => answer.to_string(),
        (true, true) => "Empty FAQ entry.".to_string(),
    }
}

fn answer_faq_query(query: &str) -> String {
    let rows = enabled_faq_rows();
    if rows.is_empty() {
        return "No Server FAQs are currently available.".to_string();
    }
    let q = query.trim();
    if q.eq_ignore_ascii_case("list") || q.eq_ignore_ascii_case("help") || q.is_empty() {
        let mut parts = vec!["Server FAQ list:".to_string()];
        for (idx, row) in rows.iter().enumerate().take(12) {
            let n = idx + 1;
            let mut preview = faq_text(row);
            if preview.len() > 80 {
                preview.truncate(77);
                preview.push_str("...");
            }
            parts.push(format!("#{n} {preview}"));
        }
        parts.push("Use ?faq #number, for example ?faq #1.".to_string());
        return parts.join(" | ");
    }
    if let Ok(n) = q.trim_start_matches('#').parse::<usize>() {
        if n >= 1 && n <= rows.len() {
            return format_faq_answer(n, &rows[n - 1]);
        }
        return format!("No Server FAQ #{n}. Use ?faq list.");
    }
    "Use ?faq list, then ask by number like ?faq #1.".to_string()
}

fn format_faq_answer(number: usize, row: &serde_json::Value) -> String {
    format!("[FAQ #{number}] {}", faq_text(row))
}

fn faq_number_for_id(id: &str) -> Option<usize> {
    if id.is_empty() { return None; }
    enabled_faq_rows().iter().position(|row| row.get("id").and_then(|v| v.as_str()) == Some(id)).map(|i| i + 1)
}

fn faq_id_for_number(number: usize) -> Option<String> {
    if number == 0 { return None; }
    enabled_faq_rows()
        .get(number - 1)
        .and_then(|row| row.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
}

fn set_faq_number(number: usize, text: &str, user: &str) -> String {
    let Some(id) = faq_id_for_number(number) else {
        return format!("No Server FAQ #{number}. Use ?faq list.");
    };
    match storage::public_update("faqs", &id, serde_json::json!({
        "text": text.trim(),
        "category": "Edited by Command",
        "enabled": true,
        "source": format!("Minecraft command from {user}")
    }), user) {
        Ok(Some(_)) => format!("Server FAQ #{number} updated."),
        Ok(None) => format!("No Server FAQ #{number}."),
        Err(e) => format!("FAQ update failed: {e}"),
    }
}

fn delete_faq_number(number: usize, user: &str) -> String {
    let Some(id) = faq_id_for_number(number) else {
        return format!("No Server FAQ #{number}. Use ?faq list.");
    };
    match storage::public_delete("faqs", &id, user) {
        Ok(true) => format!("Server FAQ #{number} deleted."),
        Ok(false) => format!("No Server FAQ #{number}."),
        Err(e) => format!("FAQ delete failed: {e}"),
    }
}

fn send_configured_reply(client: &Client, mode: &str, target: &str, mesadmin: &str) {
    if mode.eq_ignore_ascii_case("chat") || mode.eq_ignore_ascii_case("public") {
        let _ = client.chat(mesadmin.to_string());
    } else {
        let _ = client.chat(format!("/msg {target} {mesadmin}"));
    }
}

fn normalize_incoming_chat_line(line: &str) -> String {
    let mut cleaned = line.trim().to_string();

    // Runtime logs and some server chat formats prepend: chat[BotName]: actual mesadmin
    // The greeter/parser must operate on the actual Minecraft line, not the log prefix.
    if cleaned.to_lowercase().starts_with("chat[") {
        if let Some((_, rest)) = cleaned.split_once(":") {
            cleaned = rest.trim().to_string();
        }
    }

    // Remove ANSI escape sequences that may be present in formatted chat.
    let mut out = String::with_capacity(cleaned.len());
    let mut chars = cleaned.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() { break; }
            }
        } else {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

fn parse_login_greeter_player(line: &str) -> Option<String> {
    let trimmed = normalize_incoming_chat_line(line);
    let trimmed = trimmed.trim();
    if trimmed.is_empty() { return None; }
    let lower = trimmed.to_lowercase();

    // Common vanilla/server join formats:
    //   Player joined the game
    //   Player joined the server.
    //   Player joined the server
    // Also works after the host log prefix is stripped.
    for marker in [" joined the game", " joined the server.", " joined the server", " joined"] {
        if let Some(idx) = lower.find(marker) {
            let candidate = trimmed[..idx].trim();
            if is_reasonable_minecraft_name(candidate) {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

fn is_reasonable_minecraft_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty()
        && n.len() <= 32
        && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn parse_whisper_sender(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if !(lower.contains("whisper") || lower.contains(" whispers ") || lower.contains("msg")) {
        return None;
    }
    infer_chat_sender(line)
}

fn parse_faq_add_command(line: &str) -> Option<(String, String)> {
    let lower = line.to_lowercase();
    let command_start = lower.find("?faqadd").or_else(|| lower.find("?addfaq"))?;
    let sender = infer_chat_sender(&line[..command_start]).unwrap_or_else(|| "Player".to_string());
    let command = line[command_start..].trim();
    let text = command.split_once(' ').map(|(_, rest)| rest.trim()).unwrap_or("").to_string();
    if text.is_empty() { return None; }
    Some((sender, text))
}

fn parse_faq_set_command(line: &str) -> Option<(String, usize, String)> {
    let lower = line.to_lowercase();
    let command_start = lower.find("?faqset").or_else(|| lower.find("?faqedit"))?;
    let sender = infer_chat_sender(&line[..command_start]).unwrap_or_else(|| "Player".to_string());
    let body = line[command_start..].trim().split_once(' ').map(|(_, rest)| rest.trim())?;
    let (num, text) = body.split_once('|')?;
    let number = num.trim().trim_start_matches('#').parse::<usize>().ok()?;
    let text = text.trim().to_string();
    if text.is_empty() { return None; }
    Some((sender, number, text))
}

fn parse_faq_delete_command(line: &str) -> Option<(String, usize)> {
    let lower = line.to_lowercase();
    let command_start = lower.find("?faqdel").or_else(|| lower.find("?faqdelete"))?;
    let sender = infer_chat_sender(&line[..command_start]).unwrap_or_else(|| "Player".to_string());
    let number_text = line[command_start..].trim().split_once(' ').map(|(_, rest)| rest.trim())?;
    let number = number_text.trim_start_matches('#').parse::<usize>().ok()?;
    Some((sender, number))
}

fn pick_login_greeter_mesadmin(player: &str) -> Option<String> {
    let rows = storage::public_load("login_greeters").unwrap_or_default();
    let player_l = player.to_lowercase();
    let mut best: Option<(i64, String)> = None;

    for row in rows {
        if !row.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true) { continue; }

        let target = row.get("target")
            .or_else(|| row.get("target_player"))
            .or_else(|| row.get("player"))
            .and_then(|v| v.as_str())
            .unwrap_or("All")
            .trim()
            .to_lowercase();
        if target != "all" && target != "*" && target != player_l { continue; }

        let msg = row.get("mesadmin")
            .or_else(|| row.get("greeting"))
            .or_else(|| row.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if msg.is_empty() { continue; }

        let priority = row.get("priority").and_then(|v| v.as_i64()).unwrap_or(0);
        let rendered = msg
            .replace("{player}", player)
            .replace("{Player}", player)
            .replace("{PLAYER}", player);
        match &best {
            Some((best_prio, _)) if *best_prio > priority => {}
            _ => best = Some((priority, rendered)),
        }
    }

    // Fallback prevents silent failure when the greeter is enabled but no entries were created yet.
    best.map(|(_, msg)| msg).or_else(|| Some(format!("Welcome, {player}. The Server remembers.")))
}

fn handle_casino_chat_command(line: &str) -> Option<(String, String)> {
    let command_start = line.find('!')?;
    let sender = infer_chat_sender(&line[..command_start])?;
    let command = line[command_start..].trim();
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let reply = match parts[0].to_lowercase().as_str() {
        "!casino" | "!creditshelp" => casino::casino_help().to_string(),
        "!credits" | "!balance" => match casino::balance(&sender) {
            Ok(balance) => format!(
                "{} has {} Credits in the Server Project treasury.",
                balance.username, balance.credits
            ),
            Err(e) => format!("Credits balance failed: {e}"),
        },
        "!slots" => {
            let Some(bet) = parse_bet(parts.get(1).copied()) else {
                return Some((sender, "Uadmin: !slots <bet>".into()));
            };
            match casino::play_slots(&sender, bet) {
                Ok(result) => format!(
                    "{} Balance: {} Credits.",
                    result.mesadmin, result.balance.credits
                ),
                Err(e) => format!("Slots failed: {e}"),
            }
        }
        "!roulette" => {
            let Some(bet) = parse_bet(parts.get(1).copied()) else {
                return Some((
                    sender,
                    "Uadmin: !roulette <bet> <red|black|green|odd|even|0-36>".into(),
                ));
            };
            let Some(choice) = parts.get(2) else {
                return Some((
                    sender,
                    "Uadmin: !roulette <bet> <red|black|green|odd|even|0-36>".into(),
                ));
            };
            match casino::play_roulette(&sender, bet, choice) {
                Ok(result) => format!(
                    "{} Balance: {} Credits.",
                    result.mesadmin, result.balance.credits
                ),
                Err(e) => format!("Roulette failed: {e}"),
            }
        }
        "!blackjack" => {
            let Some(bet) = parse_bet(parts.get(1).copied()) else {
                return Some((sender, "Uadmin: !blackjack <bet>".into()));
            };
            match casino::play_blackjack_quick(&sender, bet) {
                Ok(result) => {
                    let dealer = result
                        .dealer_total
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "?".into());
                    format!(
                        "Blackjack: {} Player [{}] = {}, Dealer [{}] = {}. Balance: {} Credits.",
                        result.mesadmin,
                        result.player_cards.join(","),
                        result.player_total,
                        result.dealer_cards.join(","),
                        dealer,
                        result.balance.credits
                    )
                }
                Err(e) => format!("Blackjack failed: {e}"),
            }
        }
        _ => return None,
    };

    Some((sender, reply))
}

fn parse_whisper_pull_command(line: &str) -> Option<(String, Option<String>)> {
    let lower = line.to_lowercase();
    if !(lower.contains("whisper") || lower.contains(" whispers ") || lower.contains("msg")) {
        return None;
    }
    let command_start = lower.find("!pull").or_else(|| lower.find(" pull"))?;
    let sender = infer_chat_sender(&line[..command_start])?;
    let command = line[command_start..].trim();
    let mut parts = command.split_whitespace();
    let verb = parts.next()?.trim_start_matches('!').to_lowercase();
    if verb != "pull" && verb != "stasis" {
        return None;
    }
    let query = parts.collect::<Vec<_>>().join(" ");
    Some((
        sender,
        if query.trim().is_empty() {
            None
        } else {
            Some(query.trim().to_string())
        },
    ))
}

fn auto_return_waypoint_for_pearl(pearl: &PearlEntry) -> Option<WaypointEntry> {
    let config = storage::load_config();
    if !config.auto_walk_home_after_pearl {
        return None;
    }
    let Some(home_id) = config.auto_walk_home_waypoint_id else {
        eprintln!("auto-walk after pearl pull is enabled, but no return waypoint is selected");
        return None;
    };
    let Some(mut home) = storage::find_waypoint(home_id) else {
        eprintln!("auto-walk after pearl pull is enabled, but selected return waypoint was not found: {home_id}");
        return None;
    };
    // The return walk should normally use the same bot that pulled the pearl.
    // If the saved waypoint belongs to another bot or has no bot set, override it with the pearl bot.
    if !pearl.bot_name.trim().is_empty() {
        home.bot_name = pearl.bot_name.clone();
    }
    Some(home)
}

fn pearl_for_whisper_pull(sender: &str, query: Option<&str>) -> Result<PearlEntry> {
    let sender_key = sender.trim().to_lowercase();
    let query_key = query.unwrap_or("").trim().to_lowercase();
    let mut matches: Vec<_> = storage::load_pearls()
        .into_iter()
        .filter(|pearl| {
            let kind = pearl.stasis_kind.trim().to_lowercase();
            let can_pull_kind = kind.is_empty() || kind == "block" || kind == "both";
            let allowed = pearl.player.eq_ignore_ascii_case(&sender_key)
                || pearl.owner_user.eq_ignore_ascii_case(&sender_key)
                || pearl
                    .allowed_users
                    .iter()
                    .any(|user| user.eq_ignore_ascii_case(&sender_key));
            let query_matches = query_key.is_empty()
                || pearl.label.eq_ignore_ascii_case(&query_key)
                || pearl.player.eq_ignore_ascii_case(&query_key)
                || pearl.label.to_lowercase().contains(&query_key);
            can_pull_kind && allowed && query_matches
        })
        .collect();
    matches.sort_by(|a, b| a.label.cmp(&b.label));
    match matches.len() {
        0 => Err(anyhow!(
            "no allowed stasis pearl matched{}",
            query
                .filter(|q| !q.trim().is_empty())
                .map(|q| format!(" '{q}'"))
                .unwrap_or_default()
        )),
        1 => Ok(matches.remove(0)),
        _ => Err(anyhow!(
            "multiple pearls matched; whisper !pull <exact label or player>"
        )),
    }
}

fn parse_bet(input: Option<&str>) -> Option<i64> {
    input?.parse::<i64>().ok()
}

fn infer_chat_sender(prefix: &str) -> Option<String> {
    let cleaned = prefix
        .replace(['<', '>', '[', ']', '(', ')', ':'], " ")
        .replace("whispers", " ")
        .replace("whispered", " ")
        .replace("from", " ")
        .replace("From", " ");
    cleaned
        .split_whitespace()
        .filter(|part| {
            part.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .last()
        .map(|s| s.to_string())
}
