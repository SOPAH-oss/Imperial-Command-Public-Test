use crate::models::{
    AppUser, BotConfig, BountyEntry, ButlerChestEntry, ButlerWaypointEntry, LedgerChestEntry,
    NewBountyEntry, NewButlerChestEntry, NewButlerWaypointEntry, NewLedgerChestEntry,
    NewPearlEntry, NewShopItemEntry, NewUserRequest, NewWaypointEntry, PearlEntry, PublicUser,
    ShopItemEntry, UpdateUserRequest, UserRole, ViewportBlock, WaypointEntry,
};
use anyhow::{anyhow, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use serde_json::Value;
use uuid::Uuid;

const CONFIG_FILE: &str = "config.json";
const PEARLS_FILE: &str = "pearls.json";
const USERS_FILE: &str = "users.json";
const WAYPOINTS_FILE: &str = "waypoints.json";
const LEDGER_CHESTS_FILE: &str = "ledger_chests.json";
const BUTLER_CHESTS_FILE: &str = "butler_chests.json";
const BUTLER_WAYPOINTS_FILE: &str = "butler_waypoints.json";
const VIEWPORT_CACHE_FILE: &str = "viewport_cache.json";
const SHOP_ITEMS_FILE: &str = "shop_items.json";
const BOUNTIES_FILE: &str = "bounties.json";

pub fn load_config() -> BotConfig {
    fs::read_to_string(CONFIG_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(config: &BotConfig) -> Result<()> {
    fs::write(CONFIG_FILE, serde_json::to_string_pretty(config)?)?;
    Ok(())
}

pub fn load_users() -> Vec<AppUser> {
    fs::read_to_string(USERS_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| {
            vec![AppUser {
                username: "admin".into(),
                password: "change-me".into(),
                role: UserRole::Owner,
                minecraft_name: "admin".into(),
                discord_name: String::new(),
                permissions: vec!["bank".into(), "bounty".into(), "book_writer".into(), "shop".into(), "faq_whisper".into()],
            }]
        })
}

pub fn save_users(users: &[AppUser]) -> Result<()> {
    fs::write(USERS_FILE, serde_json::to_string_pretty(users)?)?;
    Ok(())
}

pub fn ensure_users_file() -> Result<()> {
    if !std::path::Path::new(USERS_FILE).exists() {
        save_users(&load_users())?;
    }
    Ok(())
}

pub fn public_users() -> Vec<PublicUser> {
    load_users()
        .into_iter()
        .map(|u| PublicUser {
            username: u.username,
            role: u.role,
            minecraft_name: u.minecraft_name,
            discord_name: u.discord_name,
            permissions: u.permissions,
        })
        .collect()
}

pub fn add_user(input: NewUserRequest) -> Result<PublicUser> {
    let username = input.username.trim().to_lowercase();
    if username.is_empty() || input.password.is_empty() {
        return Err(anyhow!("username and password are required"));
    }
    let mut users = load_users();
    if users
        .iter()
        .any(|u| u.username.eq_ignore_ascii_case(&username))
    {
        return Err(anyhow!("user already exists"));
    }
    let user = AppUser {
        username: username.clone(),
        password: input.password,
        role: input.role,
        minecraft_name: input.minecraft_name.unwrap_or_default(),
        discord_name: input.discord_name.unwrap_or_default(),
        permissions: normalize_permissions(input.permissions),
    };
    let public = PublicUser {
        username: user.username.clone(),
        role: user.role.clone(),
        minecraft_name: user.minecraft_name.clone(),
        discord_name: user.discord_name.clone(),
        permissions: user.permissions.clone(),
    };
    users.push(user);
    save_users(&users)?;
    Ok(public)
}

pub fn update_user(username: &str, input: UpdateUserRequest) -> Result<Option<PublicUser>> {
    let username = username.trim().to_lowercase();
    let mut users = load_users();
    let Some(existing) = users
        .iter_mut()
        .find(|u| u.username.eq_ignore_ascii_case(&username))
    else {
        return Ok(None);
    };

    if !username.eq_ignore_ascii_case("admin") {
        if let Some(role) = input.role {
            existing.role = role;
        }
    }
    if let Some(password) = input.password {
        if !password.is_empty() {
            existing.password = password;
        }
    }
    if let Some(minecraft_name) = input.minecraft_name {
        existing.minecraft_name = minecraft_name;
    }
    if let Some(discord_name) = input.discord_name {
        existing.discord_name = discord_name;
    }
    if let Some(permissions) = input.permissions {
        existing.permissions = normalize_permissions(permissions);
    }
    let public = PublicUser {
        username: existing.username.clone(),
        role: existing.role.clone(),
        minecraft_name: existing.minecraft_name.clone(),
        discord_name: existing.discord_name.clone(),
        permissions: existing.permissions.clone(),
    };
    save_users(&users)?;
    Ok(Some(public))
}

pub fn delete_user(username: &str) -> Result<bool> {
    let username = username.trim().to_lowercase();
    if username.eq_ignore_ascii_case("admin") {
        return Err(anyhow!(
            "the primary admin owner account cannot be deleted"
        ));
    }
    let mut users = load_users();
    let old = users.len();
    users.retain(|u| !u.username.eq_ignore_ascii_case(&username));
    save_users(&users)?;
    Ok(old != users.len())
}

pub fn find_user(username: &str) -> Option<AppUser> {
    load_users()
        .into_iter()
        .find(|u| u.username.eq_ignore_ascii_case(username.trim()))
}

pub fn verify_user(username: &str, password: &str) -> Option<AppUser> {
    find_user(username).filter(|u| u.password == password)
}

pub fn normalize_permissions(permissions: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for p in permissions {
        let key = p.trim().to_lowercase();
        if matches!(key.as_str(), "bank" | "bounty" | "book_writer" | "shop") && !out.contains(&key) {
            out.push(key);
        }
    }
    out
}

pub fn load_pearls() -> Vec<PearlEntry> {
    fs::read_to_string(PEARLS_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_pearls(pearls: &[PearlEntry]) -> Result<()> {
    fs::write(PEARLS_FILE, serde_json::to_string_pretty(pearls)?)?;
    Ok(())
}

pub fn visible_pearls(username: &str, role: &UserRole) -> Vec<PearlEntry> {
    let username = username.trim().to_lowercase();
    load_pearls()
        .into_iter()
        .filter(|p| {
            *role == UserRole::Owner
                || p.owner_user.eq_ignore_ascii_case(&username)
                || p.player.eq_ignore_ascii_case(&username)
                || p.allowed_users
                    .iter()
                    .any(|u| u.eq_ignore_ascii_case(&username))
        })
        .collect()
}

pub fn add_pearl(input: NewPearlEntry, owner_user: String, role: &UserRole) -> Result<PearlEntry> {
    let mut pearls = load_pearls();
    let owner_user = owner_user.trim().to_lowercase();
    let player = if *role == UserRole::Owner {
        input.player
    } else {
        owner_user.clone()
    };
    let pearl = PearlEntry {
        id: Uuid::new_v4(),
        player,
        label: input.label,
        category: normalize_waypoint_category(&input.category),
        stasis_kind: if input.stasis_kind.trim().is_empty() {
            "block".into()
        } else {
            input.stasis_kind.trim().to_lowercase()
        },
        item_name: input.item_name.trim().to_string(),
        inventory_slot: input.inventory_slot.min(35),
        bot_name: input.bot_name.trim().to_string(),
        x: input.x,
        y: input.y,
        z: input.z,
        notes: input.notes.unwrap_or_default(),
        created_at: Utc::now().to_rfc3339(),
        owner_user,
        allowed_users: input
            .allowed_users
            .into_iter()
            .map(|u| u.trim().to_lowercase())
            .filter(|u| !u.is_empty())
            .collect(),
    };
    pearls.push(pearl.clone());
    save_pearls(&pearls)?;
    Ok(pearl)
}

pub fn update_pearl(
    id: Uuid,
    input: NewPearlEntry,
    username: &str,
    role: &UserRole,
) -> Result<Option<PearlEntry>> {
    let mut pearls = load_pearls();
    let username = username.trim().to_lowercase();
    let Some(existing) = pearls.iter_mut().find(|p| p.id == id) else {
        return Ok(None);
    };
    if !(*role == UserRole::Owner || existing.owner_user.eq_ignore_ascii_case(&username)) {
        return Ok(None);
    }
    existing.player = if *role == UserRole::Owner {
        input.player.trim().to_string()
    } else {
        existing.owner_user.clone()
    };
    existing.label = input.label.trim().to_string();
    existing.category = normalize_waypoint_category(&input.category);
    existing.stasis_kind = if input.stasis_kind.trim().is_empty() {
        "block".into()
    } else {
        input.stasis_kind.trim().to_lowercase()
    };
    existing.item_name = input.item_name.trim().to_string();
    existing.inventory_slot = input.inventory_slot.min(35);
    existing.bot_name = input.bot_name.trim().to_string();
    existing.x = input.x;
    existing.y = input.y;
    existing.z = input.z;
    existing.notes = input.notes.unwrap_or_default();
    existing.allowed_users = input
        .allowed_users
        .into_iter()
        .map(|u| u.trim().to_lowercase())
        .filter(|u| !u.is_empty())
        .collect();
    let updated = existing.clone();
    save_pearls(&pearls)?;
    Ok(Some(updated))
}

pub fn delete_pearl(id: Uuid, username: &str, role: &UserRole) -> Result<bool> {
    let mut pearls = load_pearls();
    let old = pearls.len();
    let username = username.trim().to_lowercase();
    pearls.retain(|p| {
        if p.id != id {
            return true;
        }
        !(*role == UserRole::Owner || p.owner_user.eq_ignore_ascii_case(&username))
    });
    save_pearls(&pearls)?;
    Ok(old != pearls.len())
}

pub fn find_visible_pearl(id: Uuid, username: &str, role: &UserRole) -> Option<PearlEntry> {
    let username = username.trim().to_lowercase();
    load_pearls().into_iter().find(|p| {
        p.id == id
            && (*role == UserRole::Owner
                || p.owner_user.eq_ignore_ascii_case(&username)
                || p.player.eq_ignore_ascii_case(&username)
                || p.allowed_users
                    .iter()
                    .any(|u| u.eq_ignore_ascii_case(&username)))
    })
}

fn normalize_waypoint_category(category: &str) -> String {
    let trimmed = category.trim();
    if trimmed.is_empty() { "Uncategorized".into() } else { trimmed.to_string() }
}

pub fn load_waypoints() -> Vec<WaypointEntry> {
    fs::read_to_string(WAYPOINTS_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_waypoints(waypoints: &[WaypointEntry]) -> Result<()> {
    fs::write(WAYPOINTS_FILE, serde_json::to_string_pretty(waypoints)?)?;
    Ok(())
}

pub fn add_waypoint(input: NewWaypointEntry) -> Result<WaypointEntry> {
    let label = input.label.trim().to_string();
    if label.is_empty() {
        return Err(anyhow!("waypoint label is required"));
    }
    let mut waypoints = load_waypoints();
    let waypoint = WaypointEntry {
        id: Uuid::new_v4(),
        label,
        category: normalize_waypoint_category(&input.category),
        bot_name: input.bot_name.trim().to_string(),
        x: input.x.floor() as i32,
        y: input.y.floor() as i32,
        z: input.z.floor() as i32,
        notes: input.notes.unwrap_or_default(),
        created_at: Utc::now().to_rfc3339(),
    };
    waypoints.push(waypoint.clone());
    save_waypoints(&waypoints)?;
    Ok(waypoint)
}

pub fn update_waypoint(id: Uuid, input: NewWaypointEntry) -> Result<Option<WaypointEntry>> {
    let label = input.label.trim().to_string();
    if label.is_empty() {
        return Err(anyhow!("waypoint label is required"));
    }
    let mut waypoints = load_waypoints();
    let Some(existing) = waypoints.iter_mut().find(|w| w.id == id) else {
        return Ok(None);
    };
    existing.label = label;
    existing.category = normalize_waypoint_category(&input.category);
    existing.bot_name = input.bot_name.trim().to_string();
    existing.x = input.x.floor() as i32;
    existing.y = input.y.floor() as i32;
    existing.z = input.z.floor() as i32;
    existing.notes = input.notes.unwrap_or_default();
    let updated = existing.clone();
    save_waypoints(&waypoints)?;
    Ok(Some(updated))
}

pub fn delete_waypoint(id: Uuid) -> Result<bool> {
    let mut waypoints = load_waypoints();
    let old = waypoints.len();
    waypoints.retain(|w| w.id != id);
    save_waypoints(&waypoints)?;
    Ok(old != waypoints.len())
}

pub fn find_waypoint(id: Uuid) -> Option<WaypointEntry> {
    load_waypoints().into_iter().find(|w| w.id == id)
}

pub fn load_ledger_chests() -> Vec<LedgerChestEntry> {
    fs::read_to_string(LEDGER_CHESTS_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_ledger_chests(entries: &[LedgerChestEntry]) -> Result<()> {
    fs::write(LEDGER_CHESTS_FILE, serde_json::to_string_pretty(entries)?)?;
    Ok(())
}

pub fn add_ledger_chest(input: NewLedgerChestEntry) -> Result<LedgerChestEntry> {
    let label = input.label.trim().to_string();
    if label.is_empty() {
        return Err(anyhow!("ledger chest label is required"));
    }
    let mut entries = load_ledger_chests();
    let entry = LedgerChestEntry {
        id: Uuid::new_v4(),
        purpose: normalize_ledger_purpose(&input.purpose),
        label,
        category: normalize_waypoint_category(&input.category),
        bot_name: input.bot_name.trim().to_string(),
        chest_x: input.chest_x,
        chest_y: input.chest_y,
        chest_z: input.chest_z,
        processed_chest_x: input.processed_chest_x,
        processed_chest_y: input.processed_chest_y,
        processed_chest_z: input.processed_chest_z,
        allowed_players: input
            .allowed_players
            .into_iter()
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty())
            .collect(),
        min_credits: input.min_credits,
        max_credits: input.max_credits,
        remove_processed_book: input.remove_processed_book,
        created_at: Utc::now().to_rfc3339(),
    };
    entries.push(entry.clone());
    save_ledger_chests(&entries)?;
    Ok(entry)
}

pub fn update_ledger_chest(
    id: Uuid,
    input: NewLedgerChestEntry,
) -> Result<Option<LedgerChestEntry>> {
    let label = input.label.trim().to_string();
    if label.is_empty() {
        return Err(anyhow!("ledger chest label is required"));
    }
    let mut entries = load_ledger_chests();
    let Some(existing) = entries.iter_mut().find(|entry| entry.id == id) else {
        return Ok(None);
    };
    existing.purpose = normalize_ledger_purpose(&input.purpose);
    existing.label = label;
    existing.category = normalize_waypoint_category(&input.category);
    existing.bot_name = input.bot_name.trim().to_string();
    existing.chest_x = input.chest_x;
    existing.chest_y = input.chest_y;
    existing.chest_z = input.chest_z;
    existing.processed_chest_x = input.processed_chest_x;
    existing.processed_chest_y = input.processed_chest_y;
    existing.processed_chest_z = input.processed_chest_z;
    existing.allowed_players = input
        .allowed_players
        .into_iter()
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
        .collect();
    existing.min_credits = input.min_credits;
    existing.max_credits = input.max_credits;
    existing.remove_processed_book = input.remove_processed_book;
    let updated = existing.clone();
    save_ledger_chests(&entries)?;
    Ok(Some(updated))
}

pub fn delete_ledger_chest(id: Uuid) -> Result<bool> {
    let mut entries = load_ledger_chests();
    let old = entries.len();
    entries.retain(|entry| entry.id != id);
    save_ledger_chests(&entries)?;
    Ok(old != entries.len())
}

pub fn find_ledger_chest(id: Uuid) -> Option<LedgerChestEntry> {
    load_ledger_chests()
        .into_iter()
        .find(|entry| entry.id == id)
}

fn normalize_ledger_purpose(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "writing" | "writer" | "book" | "books" => "writing".to_string(),
        _ => "banking".to_string(),
    }
}

pub fn load_butler_chests() -> Vec<ButlerChestEntry> {
    fs::read_to_string(BUTLER_CHESTS_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_butler_chests(entries: &[ButlerChestEntry]) -> Result<()> {
    fs::write(BUTLER_CHESTS_FILE, serde_json::to_string_pretty(entries)?)?;
    Ok(())
}

pub fn add_butler_chest(input: NewButlerChestEntry) -> Result<ButlerChestEntry> {
    let label = input.label.trim().to_string();
    if label.is_empty() {
        return Err(anyhow!("butler source chest label is required"));
    }
    let mut entries = load_butler_chests();
    let entry = ButlerChestEntry {
        id: Uuid::new_v4(),
        label,
        category: normalize_waypoint_category(&input.category),
        bot_name: input.bot_name.trim().to_string(),
        chest_x: input.chest_x,
        chest_y: input.chest_y,
        chest_z: input.chest_z,
        created_at: Utc::now().to_rfc3339(),
    };
    entries.push(entry.clone());
    save_butler_chests(&entries)?;
    Ok(entry)
}

pub fn update_butler_chest(
    id: Uuid,
    input: NewButlerChestEntry,
) -> Result<Option<ButlerChestEntry>> {
    let label = input.label.trim().to_string();
    if label.is_empty() {
        return Err(anyhow!("butler source chest label is required"));
    }
    let mut entries = load_butler_chests();
    let Some(existing) = entries.iter_mut().find(|entry| entry.id == id) else {
        return Ok(None);
    };
    existing.label = label;
    existing.category = normalize_waypoint_category(&input.category);
    existing.bot_name = input.bot_name.trim().to_string();
    existing.chest_x = input.chest_x;
    existing.chest_y = input.chest_y;
    existing.chest_z = input.chest_z;
    let updated = existing.clone();
    save_butler_chests(&entries)?;
    Ok(Some(updated))
}

pub fn delete_butler_chest(id: Uuid) -> Result<bool> {
    let mut entries = load_butler_chests();
    let old = entries.len();
    entries.retain(|entry| entry.id != id);
    save_butler_chests(&entries)?;
    Ok(old != entries.len())
}

pub fn find_butler_chest(id: Uuid) -> Option<ButlerChestEntry> {
    load_butler_chests()
        .into_iter()
        .find(|entry| entry.id == id)
}

pub fn load_butler_waypoints() -> Vec<ButlerWaypointEntry> {
    fs::read_to_string(BUTLER_WAYPOINTS_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_butler_waypoints(entries: &[ButlerWaypointEntry]) -> Result<()> {
    fs::write(
        BUTLER_WAYPOINTS_FILE,
        serde_json::to_string_pretty(entries)?,
    )?;
    Ok(())
}

pub fn add_butler_waypoint(input: NewButlerWaypointEntry) -> Result<ButlerWaypointEntry> {
    let label = input.label.trim().to_string();
    if label.is_empty() {
        return Err(anyhow!("butler waypoint label is required"));
    }
    let mut entries = load_butler_waypoints();
    let entry = ButlerWaypointEntry {
        id: Uuid::new_v4(),
        label,
        category: normalize_waypoint_category(&input.category),
        bot_name: input.bot_name.trim().to_string(),
        chest_x: input.chest_x,
        chest_y: input.chest_y,
        chest_z: input.chest_z,
        created_at: Utc::now().to_rfc3339(),
    };
    entries.push(entry.clone());
    save_butler_waypoints(&entries)?;
    Ok(entry)
}

pub fn update_butler_waypoint(
    id: Uuid,
    input: NewButlerWaypointEntry,
) -> Result<Option<ButlerWaypointEntry>> {
    let label = input.label.trim().to_string();
    if label.is_empty() {
        return Err(anyhow!("butler waypoint label is required"));
    }
    let mut entries = load_butler_waypoints();
    let Some(existing) = entries.iter_mut().find(|entry| entry.id == id) else {
        return Ok(None);
    };
    existing.label = label;
    existing.category = normalize_waypoint_category(&input.category);
    existing.bot_name = input.bot_name.trim().to_string();
    existing.chest_x = input.chest_x;
    existing.chest_y = input.chest_y;
    existing.chest_z = input.chest_z;
    let updated = existing.clone();
    save_butler_waypoints(&entries)?;
    Ok(Some(updated))
}

pub fn delete_butler_waypoint(id: Uuid) -> Result<bool> {
    let mut entries = load_butler_waypoints();
    let old = entries.len();
    entries.retain(|entry| entry.id != id);
    save_butler_waypoints(&entries)?;
    Ok(old != entries.len())
}

pub fn find_butler_waypoint(id: Uuid) -> Option<ButlerWaypointEntry> {
    load_butler_waypoints()
        .into_iter()
        .find(|entry| entry.id == id)
}

pub fn load_shop_items() -> Vec<ShopItemEntry> {
    fs::read_to_string(SHOP_ITEMS_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_shop_items(entries: &[ShopItemEntry]) -> Result<()> {
    fs::write(SHOP_ITEMS_FILE, serde_json::to_string_pretty(entries)?)?;
    Ok(())
}

fn normalize_shop_reward_type(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "discord_role" | "discord-role" | "role" => "discord_role".to_string(),
        "permission" => "permission".to_string(),
        "title" => "title".to_string(),
        "custom" => "custom".to_string(),
        _ => "manual".to_string(),
    }
}

pub fn add_shop_item(input: NewShopItemEntry) -> Result<ShopItemEntry> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(anyhow!("shop item name is required"));
    }
    if input.price <= 0 {
        return Err(anyhow!("shop item price must be at least 1 Credits"));
    }
    let mut entries = load_shop_items();
    let entry = ShopItemEntry {
        id: Uuid::new_v4(),
        name,
        description: input.description.trim().to_string(),
        price: input.price,
        command_hint: input.command_hint.trim().to_string(),
        reward_type: normalize_shop_reward_type(&input.reward_type),
        discord_role_id: input.discord_role_id.trim().to_string(),
        discord_role_name: input.discord_role_name.trim().to_string(),
        enabled: input.enabled,
        created_at: Utc::now().to_rfc3339(),
    };
    entries.push(entry.clone());
    save_shop_items(&entries)?;
    Ok(entry)
}

pub fn update_shop_item(id: Uuid, input: NewShopItemEntry) -> Result<Option<ShopItemEntry>> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(anyhow!("shop item name is required"));
    }
    if input.price <= 0 {
        return Err(anyhow!("shop item price must be at least 1 Credits"));
    }
    let mut entries = load_shop_items();
    let Some(existing) = entries.iter_mut().find(|entry| entry.id == id) else {
        return Ok(None);
    };
    existing.name = name;
    existing.description = input.description.trim().to_string();
    existing.price = input.price;
    existing.command_hint = input.command_hint.trim().to_string();
    existing.reward_type = normalize_shop_reward_type(&input.reward_type);
    existing.discord_role_id = input.discord_role_id.trim().to_string();
    existing.discord_role_name = input.discord_role_name.trim().to_string();
    existing.enabled = input.enabled;
    let updated = existing.clone();
    save_shop_items(&entries)?;
    Ok(Some(updated))
}

pub fn delete_shop_item(id: Uuid) -> Result<bool> {
    let mut entries = load_shop_items();
    let old = entries.len();
    entries.retain(|entry| entry.id != id);
    save_shop_items(&entries)?;
    Ok(old != entries.len())
}

pub fn load_bounties() -> Vec<BountyEntry> {
    fs::read_to_string(BOUNTIES_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_bounties(entries: &[BountyEntry]) -> Result<()> {
    fs::write(BOUNTIES_FILE, serde_json::to_string_pretty(entries)?)?;
    Ok(())
}

pub fn add_bounty(input: NewBountyEntry, posted_by: String) -> Result<BountyEntry> {
    let target = input.target.trim().to_string();
    if target.is_empty() {
        return Err(anyhow!("bounty target is required"));
    }
    if input.reward <= 0 {
        return Err(anyhow!("bounty reward must be at least 1 Credits"));
    }
    let mut entries = load_bounties();
    let entry = BountyEntry {
        id: Uuid::new_v4(),
        target,
        reward: input.reward,
        posted_by: posted_by.trim().to_string(),
        note: input.note.trim().to_string(),
        status: normalize_bounty_status(&input.status),
        created_at: Utc::now().to_rfc3339(),
    };
    entries.push(entry.clone());
    save_bounties(&entries)?;
    Ok(entry)
}

pub fn update_bounty(
    id: Uuid,
    input: NewBountyEntry,
    posted_by: String,
) -> Result<Option<BountyEntry>> {
    let target = input.target.trim().to_string();
    if target.is_empty() {
        return Err(anyhow!("bounty target is required"));
    }
    if input.reward <= 0 {
        return Err(anyhow!("bounty reward must be at least 1 Credits"));
    }
    let mut entries = load_bounties();
    let Some(existing) = entries.iter_mut().find(|entry| entry.id == id) else {
        return Ok(None);
    };
    existing.target = target;
    existing.reward = input.reward;
    existing.posted_by = posted_by.trim().to_string();
    existing.note = input.note.trim().to_string();
    existing.status = normalize_bounty_status(&input.status);
    let updated = existing.clone();
    save_bounties(&entries)?;
    Ok(Some(updated))
}

pub fn delete_bounty(id: Uuid) -> Result<bool> {
    let mut entries = load_bounties();
    let old = entries.len();
    entries.retain(|entry| entry.id != id);
    save_bounties(&entries)?;
    Ok(old != entries.len())
}

fn normalize_bounty_status(value: &str) -> String {
    match value
        .trim()
        .to_lowercase()
        .replace(['_', '-'], " ")
        .as_str()
    {
        "progress" | "in progress" | "active" => "in_progress".to_string(),
        "complete" | "completed" | "done" | "claimed" => "completed".to_string(),
        "cancel" | "cancelled" | "canceled" => "cancelled".to_string(),
        _ => "open".to_string(),
    }
}

pub fn load_viewport_cache() -> Vec<ViewportBlock> {
    fs::read_to_string(VIEWPORT_CACHE_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_viewport_cache(blocks: &[ViewportBlock]) -> Result<()> {
    fs::write(VIEWPORT_CACHE_FILE, serde_json::to_string_pretty(blocks)?)?;
    Ok(())
}

pub fn merge_viewport_cache(blocks: &[ViewportBlock]) -> Result<()> {
    if blocks.is_empty() {
        return Ok(());
    }
    let mut by_column: HashMap<(i32, i32), ViewportBlock> = load_viewport_cache()
        .into_iter()
        .map(|block| ((block.x, block.z), block))
        .collect();
    for block in blocks {
        by_column.insert((block.x, block.z), block.clone());
    }
    let mut merged: Vec<_> = by_column.into_values().collect();
    merged.sort_by_key(|block| (block.x, block.z));
    save_viewport_cache(&merged)
}

pub fn cached_viewport_blocks(
    min_x: i32,
    max_x: i32,
    min_z: i32,
    max_z: i32,
) -> Vec<ViewportBlock> {
    load_viewport_cache()
        .into_iter()
        .filter(|block| {
            block.x >= min_x && block.x <= max_x && block.z >= min_z && block.z <= max_z
        })
        .collect()
}


pub fn public_collections() -> Vec<&'static str> {
    vec![
        "chronicles","library","decrees","historical_events","timeline","archive_backups",
        "players","factions","intelligence_reports","sightings","watchlist","contacts",
        "regions","roads","portals","ice_highways","routes","regional_ownership",
        "admins","former_admins","former_archadmins","founders","benefactors","lineage","mentors",
        "relics","artifacts","banners","codices","wonders",
        "proposals","votes","decisions","census","citizens","ranks","promotions",
        "mail","notifications","news","discoveries","explorer_rankings","research","scholar_contributions",
        "id_cards","certificates","archive_scans","pearl_monitor","assets","emergency_log",
        "departments","budgets","financial_reports","faqs","login_greeters"
    ]
}

fn public_file(collection: &str) -> String {
    format!("public_{}.json", collection.replace('/', "_"))
}

pub fn public_load(collection: &str) -> Result<Vec<Value>> {
    let file = public_file(collection);
    if !std::path::Path::new(&file).exists() {
        fs::write(&file, "[]")?;
    }
    let s = fs::read_to_string(&file).unwrap_or_else(|_| "[]".into());
    Ok(serde_json::from_str(&s).unwrap_or_default())
}

pub fn public_save(collection: &str, rows: &[Value]) -> Result<()> {
    fs::write(public_file(collection), serde_json::to_string_pretty(rows)?)?;
    Ok(())
}

fn public_touch(mut row: Value, user: &str, created: bool) -> Value {
    let now = Utc::now().to_rfc3339();
    if !row.is_object() { row = serde_json::json!({"value": row}); }
    let obj = row.as_object_mut().unwrap();
    if created {
        obj.entry("id").or_insert_with(|| serde_json::json!(Uuid::new_v4().to_string()));
        obj.entry("created_at").or_insert_with(|| serde_json::json!(now.clone()));
        obj.entry("created_by").or_insert_with(|| serde_json::json!(user));
    }
    obj.insert("updated_at".into(), serde_json::json!(now));
    obj.insert("updated_by".into(), serde_json::json!(user));
    row
}

pub fn public_add(collection: &str, row: Value, user: &str) -> Result<Value> {
    let mut rows = public_load(collection)?;
    let row = public_touch(row, user, true);
    rows.push(row.clone());
    public_save(collection, &rows)?;
    Ok(row)
}

pub fn public_update(collection: &str, id: &str, row: Value, user: &str) -> Result<Option<Value>> {
    let mut rows = public_load(collection)?;
    for existing in rows.iter_mut() {
        if existing.get("id").and_then(|v| v.as_str()) == Some(id) {
            let mut row = public_touch(row, user, false);
            if let Some(obj) = row.as_object_mut() { obj.insert("id".into(), serde_json::json!(id)); }
            *existing = row.clone();
            public_save(collection, &rows)?;
            return Ok(Some(row));
        }
    }
    Ok(None)
}

pub fn public_delete(collection: &str, id: &str, user: &str) -> Result<bool> {
    let mut rows = public_load(collection)?;
    let old_len = rows.len();
    rows.retain(|r| r.get("id").and_then(|v| v.as_str()) != Some(id));
    if rows.len() != old_len {
        public_save(collection, &rows)?;
        let _ = public_add("emergency_log", serde_json::json!({"kind":"delete","collection":collection,"record_id":id}), user);
        Ok(true)
    } else { Ok(false) }
}
