//! Teleport bookmarks and reading a scroll toward one (doc/TELEPORTATION.md).

use super::{auth_db, land::plot_key, GameState};
use crate::auth::AuthService;
use crate::item_defs::{ItemEffect, UseEffect};
use crate::types::{PlayerId, Position, ServerMessage};
use onlinerpg_shared::bookmark::{clean_name, BookmarkError};
use onlinerpg_terrain::land::plot_addr;

impl GameState {
    pub async fn send_bookmarks(&self, player_id: &PlayerId, auth: &AuthService) {
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let db = auth.clone();
        match auth_db(move || db.bookmarks(character_id)).await {
            Ok(bookmarks) => {
                self.send_direct_message(player_id, ServerMessage::Bookmarks { bookmarks })
                    .await
            }
            Err(error) => tracing::warn!(%error, "Failed to load bookmarks"),
        }
    }

    pub async fn save_bookmark(&self, player_id: &PlayerId, name: String, auth: &AuthService) {
        let result = self.try_save_bookmark(player_id, &name, auth).await;
        self.finish_bookmark_change(player_id, result, auth).await;
    }

    pub async fn rename_bookmark(
        &self,
        player_id: &PlayerId,
        id: i64,
        name: String,
        auth: &AuthService,
    ) {
        let result = async {
            let name = clean_name(&name).ok_or(BookmarkError::Name)?;
            let character_id = self.bookmark_owner(player_id).await?;
            let db = auth.clone();
            bookmark_db(move || db.rename_bookmark(character_id, id, &name)).await?
        }
        .await;
        self.finish_bookmark_change(player_id, result, auth).await;
    }

    pub async fn delete_bookmark(&self, player_id: &PlayerId, id: i64, auth: &AuthService) {
        let result = async {
            let character_id = self.bookmark_owner(player_id).await?;
            let db = auth.clone();
            bookmark_db(move || db.delete_bookmark(character_id, id)).await?
        }
        .await;
        self.finish_bookmark_change(player_id, result, auth).await;
    }

    /// Show the departure, then read the scroll toward the bookmark. A
    /// request that needs no lookup to refuse never departs.
    pub async fn request_bookmark_teleport(
        &self,
        player_id: PlayerId,
        instance_id: u64,
        bookmark_id: i64,
        auth: &std::sync::Arc<AuthService>,
    ) {
        if let Err(error) = self.may_aim_teleport(&player_id, instance_id).await {
            self.refuse_bookmark(&player_id, error).await;
            self.cancel_teleport_effect(&player_id).await;
            return;
        }
        let auth = std::sync::Arc::clone(auth);
        self.depart_then(player_id, move |game| async move {
            game.teleport_to_bookmark(&player_id, instance_id, bookmark_id, &auth)
                .await;
        });
    }

    /// Read a teleport scroll toward a bookmark. A refusal keeps the scroll.
    pub async fn teleport_to_bookmark(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
        bookmark_id: i64,
        auth: &AuthService,
    ) {
        let result = self
            .try_teleport_to_bookmark(player_id, instance_id, bookmark_id, auth)
            .await;
        if let Err(error) = result {
            self.refuse_bookmark(player_id, error).await;
            self.cancel_teleport_effect(player_id).await;
        }
    }

    async fn try_save_bookmark(
        &self,
        player_id: &PlayerId,
        name: &str,
        auth: &AuthService,
    ) -> Result<(), BookmarkError> {
        let name = clean_name(name).ok_or(BookmarkError::Name)?;
        let character_id = self.bookmark_owner(player_id).await?;
        let (position, _, floor_level) = self
            .get_player_position(player_id)
            .await
            .ok_or(BookmarkError::Failed)?;
        if floor_level != 0 {
            return Err(BookmarkError::NotSurface);
        }
        let landing = self
            .bookmark_landing(character_id, position.x, position.z, auth)
            .await?;
        let db = auth.clone();
        bookmark_db(move || db.save_bookmark(character_id, &name, landing.x, landing.z)).await?
    }

    async fn try_teleport_to_bookmark(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
        bookmark_id: i64,
        auth: &AuthService,
    ) -> Result<(), BookmarkError> {
        self.may_aim_teleport(player_id, instance_id).await?;
        let character_id = self.bookmark_owner(player_id).await?;
        let db = auth.clone();
        let bookmark = bookmark_db(move || db.bookmark(character_id, bookmark_id))
            .await?
            .ok_or(BookmarkError::Unknown)?;
        let landing = self
            .bookmark_landing(character_id, bookmark.x, bookmark.z, auth)
            .await?;
        if self
            .read_teleport_scroll(player_id, instance_id, landing, 0)
            .await
        {
            Ok(())
        } else {
            Err(BookmarkError::Failed)
        }
    }

    /// The checks that need no lookup, so a doomed request is refused
    /// before its departure is shown.
    async fn may_aim_teleport(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
    ) -> Result<(), BookmarkError> {
        if !self.controls_teleport(player_id, instance_id).await {
            return Err(BookmarkError::NoControl);
        }
        if self
            .players
            .read()
            .await
            .get(player_id)
            .is_none_or(Self::in_combat)
        {
            return Err(BookmarkError::Combat);
        }
        Ok(())
    }

    /// The one rule for saving a bookmark and for arriving at it: a spot a
    /// random teleport could land on, outside anyone else's estate.
    async fn bookmark_landing(
        &self,
        character_id: i64,
        x: f32,
        z: f32,
        auth: &AuthService,
    ) -> Result<Position, BookmarkError> {
        let landing = self
            .teleport_surface_landing(x, z)
            .await
            .ok_or(BookmarkError::NoLanding)?;
        let plot = plot_key(plot_addr(x, z));
        let db = auth.clone();
        if bookmark_db(move || db.plot_is_foreign(character_id, plot)).await? {
            return Err(BookmarkError::Owned);
        }
        Ok(landing)
    }

    /// Whether the bag item is a teleport scroll its reader may aim: the
    /// scroll or something worn carries teleport control.
    async fn controls_teleport(&self, player_id: &PlayerId, instance_id: u64) -> bool {
        let controls = |def: &crate::item_defs::ItemDefinition| {
            def.effects().contains(&ItemEffect::TeleportControl)
        };
        let inventories = self.inventories.read().await;
        inventories.get(player_id).is_some_and(|inv| {
            inv.bag
                .iter()
                .find(|item| item.instance_id == instance_id && item.quantity > 0)
                .and_then(|item| self.item_defs.get(&item.item_def_id))
                .is_some_and(|def| {
                    matches!(def.use_effect(), Some(UseEffect::TeleportRandom))
                        && (controls(def) || self.equipped_defs(inv).any(controls))
                })
        })
    }

    async fn bookmark_owner(&self, player_id: &PlayerId) -> Result<i64, BookmarkError> {
        self.character_id_of(player_id)
            .await
            .ok_or(BookmarkError::Failed)
    }

    async fn finish_bookmark_change(
        &self,
        player_id: &PlayerId,
        result: Result<(), BookmarkError>,
        auth: &AuthService,
    ) {
        match result {
            Ok(()) => self.send_bookmarks(player_id, auth).await,
            Err(error) => self.refuse_bookmark(player_id, error).await,
        }
    }

    async fn refuse_bookmark(&self, player_id: &PlayerId, error: BookmarkError) {
        self.send_direct_message(player_id, ServerMessage::BookmarkRefused { error })
            .await;
    }
}

async fn bookmark_db<T, F>(query: F) -> Result<T, BookmarkError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, crate::auth::AuthError> + Send + 'static,
{
    auth_db(query).await.map_err(|error| {
        tracing::warn!(%error, "Bookmark database request failed");
        BookmarkError::Failed
    })
}
