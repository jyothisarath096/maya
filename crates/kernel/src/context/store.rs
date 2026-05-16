#![allow(dead_code)]

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};

use crate::{sched::queue, sync::TicketLock};

#[derive(Debug, Clone)]
pub struct ContextEntry {
    pub key: String,
    pub value: String,
    pub tick: u64,
    pub ttl_ticks: Option<u64>,
}

struct ContextStore {
    entries: BTreeMap<String, ContextEntry>,
}

static STORE: TicketLock<Option<ContextStore>> = TicketLock::new(None);

pub fn init() {
    *STORE.lock() = Some(ContextStore {
        entries: BTreeMap::new(),
    });
}

pub fn set(key: &str, value: &str, ttl_ticks: Option<u64>) {
    let current_tick = queue::tick_count();
    let mut guard = STORE.lock();
    let store = guard.get_or_insert_with(|| ContextStore {
        entries: BTreeMap::new(),
    });

    store.entries.insert(
        key.to_string(),
        ContextEntry {
            key: key.to_string(),
            value: value.to_string(),
            tick: current_tick,
            ttl_ticks,
        },
    );
}

pub fn get(key: &str) -> Option<String> {
    let current_tick = queue::tick_count();
    let mut guard = STORE.lock();
    let store = guard.as_mut()?;

    let expired = store
        .entries
        .get(key)
        .map(|entry| is_expired(entry, current_tick))
        .unwrap_or(false);
    if expired {
        store.entries.remove(key);
        return None;
    }

    store.entries.get(key).map(|entry| entry.value.clone())
}

pub fn delete(key: &str) {
    if let Some(store) = STORE.lock().as_mut() {
        store.entries.remove(key);
    }
}

pub fn snapshot() -> Vec<(String, String)> {
    let current_tick = queue::tick_count();
    let mut guard = STORE.lock();
    let Some(store) = guard.as_mut() else {
        return Vec::new();
    };

    store
        .entries
        .retain(|_, entry| !is_expired(entry, current_tick));

    let mut entries = Vec::with_capacity(store.entries.len());
    for entry in store.entries.values() {
        entries.push((entry.key.clone(), entry.value.clone()));
    }
    entries
}

pub fn expire(current_tick: u64) {
    if let Some(store) = STORE.lock().as_mut() {
        store
            .entries
            .retain(|_, entry| !is_expired(entry, current_tick));
    }
}

fn is_expired(entry: &ContextEntry, current_tick: u64) -> bool {
    match entry.ttl_ticks {
        None => false,
        Some(ttl_ticks) => current_tick > entry.tick.saturating_add(ttl_ticks),
    }
}
