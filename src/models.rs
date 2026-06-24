use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotAccountConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username_or_email: String,
    pub auth_mode: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    // Legacy single-bot fields kept so older config.json files still load.
    pub host: String,
    pub port: u16,
    pub username_or_email: String,
    pub auth_mode: String,

    pub gui_host: String,
    pub gui_port: u16,
    pub pull_mode: String,
    #[serde(default)]
    pub casino_public_chat: bool,
    #[serde(default)]
    pub auto_walk_home_after_pearl: bool,
    #[serde(default)]
    pub auto_walk_home_waypoint_id: Option<Uuid>,
    #[serde(default = "default_waypoint_categories")]
    pub waypoint_categories: Vec<String>,
    #[serde(default)]
    pub faq_whisper_add_enabled: bool,
    #[serde(default)]
    pub faq_whisper_cooldown_seconds: u64,
    #[serde(default = "default_faq_output_mode")]
    pub faq_output_mode: String,
    #[serde(default)]
    pub login_greeter_enabled: bool,
    #[serde(default)]
    pub login_greeter_cooldown_seconds: u64,
    #[serde(default = "default_greeter_output_mode")]
    pub greeter_output_mode: String,

    #[serde(default)]
    pub accounts: Vec<BotAccountConfig>,
}

impl BotConfig {
    pub fn normalized_accounts(&self) -> Vec<BotAccountConfig> {
        if !self.accounts.is_empty() {
            return self.accounts.clone();
        }
        vec![BotAccountConfig {
            name: "UtilityBot".into(),
            host: self.host.clone(),
            port: self.port,
            username_or_email: self.username_or_email.clone(),
            auth_mode: self.auth_mode.clone(),
            enabled: true,
        }]
    }
}

impl Default for BotConfig {
    fn default() -> Self {
        let account = BotAccountConfig {
            name: "UtilityBot".into(),
            host: "minecraft.example.org".into(),
            port: 25565,
            username_or_email: "bot@example.com".into(),
            auth_mode: "microsoft".into(),
            enabled: true,
        };
        Self {
            host: account.host.clone(),
            port: account.port,
            username_or_email: account.username_or_email.clone(),
            auth_mode: account.auth_mode.clone(),
            gui_host: "0.0.0.0".into(),
            gui_port: 8081,
            pull_mode: "block".into(),
            casino_public_chat: false,
            auto_walk_home_after_pearl: false,
            auto_walk_home_waypoint_id: None,
            waypoint_categories: default_waypoint_categories(),
            faq_whisper_add_enabled: true,
            faq_whisper_cooldown_seconds: 60,
            faq_output_mode: default_faq_output_mode(),
            login_greeter_enabled: true,
            login_greeter_cooldown_seconds: 300,
            greeter_output_mode: default_greeter_output_mode(),
            accounts: vec![account],
        }
    }
}

fn default_faq_output_mode() -> String {
    "whisper".to_string()
}

fn default_greeter_output_mode() -> String {
    "whisper".to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct BotAccountStatus {
    pub name: String,
    pub connected: bool,
    pub username: Option<String>,
    pub mesadmin: String,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorldClockStatus {
    pub day_time: i64,
    pub time_of_day: i64,
    pub time_label: String,
    pub is_day: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PearlEntry {
    pub id: Uuid,
    pub player: String,
    pub label: String,
    #[serde(default = "default_waypoint_category")]
    pub category: String,
    #[serde(default = "default_stasis_kind")]
    pub stasis_kind: String,
    #[serde(default)]
    pub item_name: String,
    #[serde(default, alias = "hotbar_slot")]
    pub inventory_slot: u8,
    #[serde(default)]
    pub bot_name: String,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub notes: String,
    pub created_at: String,
    #[serde(default)]
    pub owner_user: String,
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewPearlEntry {
    pub player: String,
    pub label: String,
    #[serde(default = "default_waypoint_category")]
    pub category: String,
    #[serde(default = "default_stasis_kind")]
    pub stasis_kind: String,
    #[serde(default)]
    pub item_name: String,
    #[serde(default, alias = "hotbar_slot")]
    pub inventory_slot: u8,
    #[serde(default)]
    pub bot_name: String,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub notes: Option<String>,
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaypointEntry {
    pub id: Uuid,
    pub label: String,
    #[serde(default = "default_waypoint_category")]
    pub category: String,
    #[serde(default)]
    pub bot_name: String,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    #[serde(default)]
    pub notes: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewWaypointEntry {
    pub label: String,
    #[serde(default = "default_waypoint_category")]
    pub category: String,
    #[serde(default)]
    pub bot_name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalkToWaypointResponse {
    pub mesadmin: String,
    pub bot_name: String,
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct BotStatus {
    pub connected: bool,
    pub username: Option<String>,
    pub mesadmin: String,
    pub login_help: Option<String>,
    pub bots: Vec<BotAccountStatus>,
    pub world_time: Option<WorldClockStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Owner,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUser {
    pub username: String,
    pub password: String,
    pub role: UserRole,
    #[serde(default)]
    pub minecraft_name: String,
    #[serde(default)]
    pub discord_name: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoginResponse {
    pub username: String,
    pub role: UserRole,
    pub minecraft_name: String,
    pub discord_name: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewUserRequest {
    pub username: String,
    pub password: String,
    pub role: UserRole,
    pub minecraft_name: Option<String>,
    pub discord_name: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicUser {
    pub username: String,
    pub role: UserRole,
    pub minecraft_name: String,
    pub discord_name: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateUserRequest {
    pub password: Option<String>,
    pub role: Option<UserRole>,
    pub minecraft_name: Option<String>,
    pub discord_name: Option<String>,
    pub permissions: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CasinoBalance {
    pub username: String,
    pub credits: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBankBalanceRequest {
    pub username: String,
    pub credits: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouletteRequest {
    pub bet: i64,
    pub choice: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouletteResponse {
    pub number: u8,
    pub color: String,
    pub won: bool,
    pub payout: i64,
    pub mesadmin: String,
    pub balance: CasinoBalance,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlotsRequest {
    pub bet: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlotsResponse {
    pub reels: Vec<String>,
    pub won: bool,
    pub payout: i64,
    pub mesadmin: String,
    pub balance: CasinoBalance,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlackjackStartRequest {
    pub bet: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlackjackResponse {
    pub player_cards: Vec<String>,
    pub dealer_cards: Vec<String>,
    pub player_total: u8,
    pub dealer_total: Option<u8>,
    pub finished: bool,
    pub outcome: String,
    pub mesadmin: String,
    pub balance: CasinoBalance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShopItemEntry {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub price: i64,
    #[serde(default)]
    pub command_hint: String,
    #[serde(default = "default_shop_reward_type")]
    pub reward_type: String,
    #[serde(default)]
    pub discord_role_id: String,
    #[serde(default)]
    pub discord_role_name: String,
    #[serde(default)]
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewShopItemEntry {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub price: i64,
    #[serde(default)]
    pub command_hint: String,
    #[serde(default = "default_shop_reward_type")]
    pub reward_type: String,
    #[serde(default)]
    pub discord_role_id: String,
    #[serde(default)]
    pub discord_role_name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_shop_reward_type() -> String {
    "manual".to_string()
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BountyEntry {
    pub id: Uuid,
    pub target: String,
    pub reward: i64,
    #[serde(default)]
    pub posted_by: String,
    #[serde(default)]
    pub note: String,
    #[serde(default = "default_bounty_status")]
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewBountyEntry {
    pub target: String,
    pub reward: i64,
    #[serde(default)]
    pub note: String,
    #[serde(default = "default_bounty_status")]
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VipTableEntry {
    pub rank: usize,
    pub username: String,
    pub credits: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChestLedgerConfig {
    #[serde(default)]
    pub bot_name: String,
    #[serde(default)]
    pub label: String,
    pub chest_x: i32,
    pub chest_y: i32,
    pub chest_z: i32,
    #[serde(default, alias = "destroy_chest_x")]
    pub processed_chest_x: Option<i32>,
    #[serde(default, alias = "destroy_chest_y")]
    pub processed_chest_y: Option<i32>,
    #[serde(default, alias = "destroy_chest_z")]
    pub processed_chest_z: Option<i32>,
    #[serde(default)]
    pub allowed_players: Vec<String>,
    #[serde(default = "default_min_credits")]
    pub min_credits: i64,
    #[serde(default = "default_max_credits")]
    pub max_credits: i64,
    #[serde(default)]
    pub remove_processed_book: bool,
}

impl Default for ChestLedgerConfig {
    fn default() -> Self {
        Self {
            bot_name: "UtilityBot".into(),
            label: "Credits Bank".into(),
            chest_x: 0,
            chest_y: 64,
            chest_z: 0,
            processed_chest_x: None,
            processed_chest_y: None,
            processed_chest_z: None,
            allowed_players: vec![],
            min_credits: 1,
            max_credits: 100_000_000,
            remove_processed_book: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerChestEntry {
    pub id: Uuid,
    #[serde(default = "default_ledger_purpose")]
    pub purpose: String,
    pub label: String,
    #[serde(default = "default_waypoint_category")]
    pub category: String,
    #[serde(default)]
    pub bot_name: String,
    pub chest_x: i32,
    pub chest_y: i32,
    pub chest_z: i32,
    #[serde(default, alias = "destroy_chest_x")]
    pub processed_chest_x: Option<i32>,
    #[serde(default, alias = "destroy_chest_y")]
    pub processed_chest_y: Option<i32>,
    #[serde(default, alias = "destroy_chest_z")]
    pub processed_chest_z: Option<i32>,
    #[serde(default)]
    pub allowed_players: Vec<String>,
    #[serde(default = "default_min_credits")]
    pub min_credits: i64,
    #[serde(default = "default_max_credits")]
    pub max_credits: i64,
    #[serde(default)]
    pub remove_processed_book: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewLedgerChestEntry {
    #[serde(default = "default_ledger_purpose")]
    pub purpose: String,
    pub label: String,
    #[serde(default = "default_waypoint_category")]
    pub category: String,
    #[serde(default)]
    pub bot_name: String,
    pub chest_x: i32,
    pub chest_y: i32,
    pub chest_z: i32,
    #[serde(default, alias = "destroy_chest_x")]
    pub processed_chest_x: Option<i32>,
    #[serde(default, alias = "destroy_chest_y")]
    pub processed_chest_y: Option<i32>,
    #[serde(default, alias = "destroy_chest_z")]
    pub processed_chest_z: Option<i32>,
    #[serde(default)]
    pub allowed_players: Vec<String>,
    #[serde(default = "default_min_credits")]
    pub min_credits: i64,
    #[serde(default = "default_max_credits")]
    pub max_credits: i64,
    #[serde(default)]
    pub remove_processed_book: bool,
}

impl From<LedgerChestEntry> for ChestLedgerConfig {
    fn from(entry: LedgerChestEntry) -> Self {
        Self {
            bot_name: entry.bot_name,
            label: entry.label,
            chest_x: entry.chest_x,
            chest_y: entry.chest_y,
            chest_z: entry.chest_z,
            processed_chest_x: entry.processed_chest_x,
            processed_chest_y: entry.processed_chest_y,
            processed_chest_z: entry.processed_chest_z,
            allowed_players: entry.allowed_players,
            min_credits: entry.min_credits,
            max_credits: entry.max_credits,
            remove_processed_book: entry.remove_processed_book,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChestLedgerEntry {
    pub slot: usize,
    pub author: String,
    pub recipient: String,
    pub title: String,
    pub amount: i64,
    pub page_text: String,
    pub balance: CasinoBalance,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChestLedgerResponse {
    pub mesadmin: String,
    pub processed: Vec<ChestLedgerEntry>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookWriterRequest {
    #[serde(default)]
    pub bot_name: String,
    #[serde(default)]
    pub recipient: String,
    pub chest_x: i32,
    pub chest_y: i32,
    pub chest_z: i32,
    #[serde(default)]
    pub inventory_slot: Option<u8>,
    pub title: String,
    pub pages: Vec<String>,
    #[serde(default = "default_place_signed_book")]
    pub place_in_chest: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BookWriterResponse {
    pub mesadmin: String,
    pub bot_name: String,
    pub title: String,
    pub page_count: usize,
    pub inventory_slot: u8,
    pub placed_in_chest: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButlerChestEntry {
    pub id: Uuid,
    pub label: String,
    #[serde(default = "default_waypoint_category")]
    pub category: String,
    #[serde(default)]
    pub bot_name: String,
    pub chest_x: i32,
    pub chest_y: i32,
    pub chest_z: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewButlerChestEntry {
    pub label: String,
    #[serde(default = "default_waypoint_category")]
    pub category: String,
    #[serde(default)]
    pub bot_name: String,
    pub chest_x: i32,
    pub chest_y: i32,
    pub chest_z: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButlerWaypointEntry {
    pub id: Uuid,
    pub label: String,
    #[serde(default = "default_waypoint_category")]
    pub category: String,
    #[serde(default)]
    pub bot_name: String,
    pub chest_x: i32,
    pub chest_y: i32,
    pub chest_z: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewButlerWaypointEntry {
    pub label: String,
    #[serde(default = "default_waypoint_category")]
    pub category: String,
    #[serde(default)]
    pub bot_name: String,
    pub chest_x: i32,
    pub chest_y: i32,
    pub chest_z: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ButlerTransferRequest {
    pub source_chest_id: Uuid,
    pub destination_waypoint_id: Uuid,
    pub shulker_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ButlerTransferResponse {
    pub mesadmin: String,
    pub bot_name: String,
    pub shulker_name: String,
    pub source_label: String,
    pub destination_label: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ViewportSnapshotRequest {
    #[serde(default)]
    pub bot_name: String,
    #[serde(default = "default_viewport_chunks")]
    pub chunks: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportBlock {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ViewportSnapshot {
    pub bot_name: String,
    pub center_x: f64,
    pub center_y: f64,
    pub center_z: f64,
    pub biome: Option<String>,
    pub day_time: Option<i64>,
    pub time_of_day: Option<i64>,
    pub time_label: String,
    pub chunks: u8,
    pub scanned_columns: usize,
    pub blocks: Vec<ViewportBlock>,
}

fn default_stasis_kind() -> String {
    "block".to_string()
}

fn default_enabled() -> bool {
    true
}

fn default_min_credits() -> i64 {
    1
}

fn default_max_credits() -> i64 {
    100_000_000
}

fn default_viewport_chunks() -> u8 {
    12
}

fn default_place_signed_book() -> bool {
    true
}

fn default_ledger_purpose() -> String {
    "banking".to_string()
}

fn default_bounty_status() -> String {
    "open".to_string()
}

fn default_waypoint_category() -> String { "Uncategorized".into() }
fn default_waypoint_categories() -> Vec<String> {
    vec![
        "Uncategorized".into(),
        "Pearl Chambers".into(),
        "Bases".into(),
        "Travel".into(),
        "Bank Butler".into(),
        "Ledger Chests".into(),
        "Book Writer".into(),
    ]
}
