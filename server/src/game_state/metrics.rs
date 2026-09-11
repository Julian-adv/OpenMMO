use crate::metrics::ConcurrentCounts;
use crate::types::ClientKind;

impl super::GameState {
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
