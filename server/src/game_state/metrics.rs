impl super::GameState {
    pub(crate) async fn concurrent_account_count(&self) -> u32 {
        let _sessions = self.lock_character_sessions().await;
        let player_ids: Vec<_> = self
            .account_sessions
            .read()
            .await
            .values()
            .filter_map(|session| session.player_id)
            .collect();
        let players = self.players.read().await;
        player_ids
            .iter()
            .filter(|id| {
                players
                    .get(id)
                    .is_some_and(|player| !player.is_official_npc)
            })
            .count() as u32
    }
}
