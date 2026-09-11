use crate::auth::{unix_now, AuthService};
use crate::metrics::{AccountActivity, ConcurrentCounts};
use crate::types::{ClientKind, PlayerId};
use tracing::warn;

impl super::GameState {
    pub(super) async fn record_item_sale(&self, item_def_id: &str, quantity: u32, gold: i64) {
        let now = unix_now();
        let timestamp = now - now.rem_euclid(crate::metrics::SAMPLE_INTERVAL_SECONDS);
        let mut pending = self.pending_item_sales.write().await;
        let sale = pending
            .entry((timestamp, item_def_id.to_owned()))
            .or_insert_with(|| crate::metrics::ItemSaleRecord {
                timestamp,
                item_def_id: item_def_id.to_owned(),
                quantity: 0,
                gold: 0,
            });
        sale.quantity += u64::from(quantity);
        sale.gold += gold;
    }

    pub(crate) async fn flush_item_sales(
        &self,
        auth: &AuthService,
        now: i64,
        include_current: bool,
    ) {
        let boundary = now - now.rem_euclid(crate::metrics::SAMPLE_INTERVAL_SECONDS);
        let sales: Vec<_> = {
            let mut pending = self.pending_item_sales.write().await;
            pending
                .extract_if(|(timestamp, _), _| include_current || *timestamp < boundary)
                .map(|(_, sale)| sale)
                .collect()
        };
        if sales.is_empty() {
            return;
        }
        let saved = sales.clone();
        let auth = auth.clone();
        if let Err(error) = super::auth_db(move || auth.record_item_sales(&saved)).await {
            warn!("Item sale snapshot failed: {error}");
            let mut pending = self.pending_item_sales.write().await;
            for sale in sales {
                pending
                    .entry((sale.timestamp, sale.item_def_id.clone()))
                    .and_modify(|existing| {
                        existing.quantity += sale.quantity;
                        existing.gold += sale.gold;
                    })
                    .or_insert(sale);
            }
        }
    }

    pub(crate) async fn begin_account_activity(
        &self,
        player_id: PlayerId,
        account_name: &str,
        auth: &AuthService,
    ) {
        if self
            .players
            .read()
            .await
            .get(&player_id)
            .is_none_or(|player| player.is_official_npc)
        {
            return;
        }
        let now = unix_now();
        let activity = self
            .account_activities
            .write()
            .await
            .entry(player_id)
            .or_insert_with(|| AccountActivity {
                id: uuid::Uuid::new_v4().to_string(),
                account_name: account_name.to_ascii_lowercase(),
                started_at: now,
                last_seen_at: now,
            })
            .clone();
        Self::save_account_activity(activity, auth).await;
    }

    pub(crate) async fn end_account_activity(&self, player_id: &PlayerId, auth: &AuthService) {
        let activity = self.account_activities.write().await.remove(player_id);
        if let Some(mut activity) = activity {
            activity.last_seen_at = unix_now().max(activity.started_at);
            Self::save_account_activity(activity, auth).await;
        }
    }

    async fn save_account_activity(activity: AccountActivity, auth: &AuthService) {
        let auth = auth.clone();
        if let Err(error) =
            super::auth_db(move || auth.record_account_activities(&[activity])).await
        {
            warn!("Account activity snapshot failed: {error}");
        }
    }

    pub(crate) async fn account_activity_snapshot(&self) -> Vec<AccountActivity> {
        let activities = self.account_activities.read().await;
        let now = unix_now();
        activities
            .values()
            .map(|activity| AccountActivity {
                last_seen_at: now.max(activity.started_at),
                ..activity.clone()
            })
            .collect()
    }

    pub(crate) async fn concurrent_account_counts(&self) -> ConcurrentCounts {
        let _sessions = self.lock_character_sessions().await;
        let player_ids: Vec<_> = self
            .account_sessions
            .read()
            .await
            .values()
            .filter_map(|session| session.player_id)
            .collect();
        let players = self.players.read().await;
        let mut counts = ConcurrentCounts::default();
        for player in player_ids.iter().filter_map(|id| players.get(id)) {
            if player.is_official_npc {
                continue;
            }
            match player.client_kind {
                ClientKind::Web => counts.web_accounts += 1,
                ClientKind::Cli => counts.agent_accounts += 1,
                ClientKind::Other | ClientKind::Unknown => counts.other_accounts += 1,
            }
        }
        counts
    }
}
