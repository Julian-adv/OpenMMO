//! Standing notices the guard captain keeps on the Aldermark board. The
//! server renews each before it expires (doc/ROAD_PATROL.md).

use super::{auth_db, road_patrol::GUARD_CAPTAIN, GameState};
use crate::auth::{AuthService, BoardKey};
use crate::npc_defs::npc_defs;
use onlinerpg_shared::notice_board::NOTICE_LIFETIME_DAYS;
use std::sync::atomic::Ordering;

pub(crate) const BOARD: BoardKey = BoardKey {
    rx: -2,
    rz: 4,
    object_id: 113,
};
/// Game days before expiry at which a notice is replaced.
const RENEW_DAYS_BEFORE_EXPIRY: i64 = 5;
const CHECK_EVERY_SECS: i64 = 3600;

pub(crate) struct StandingNotice {
    pub title: &'static str,
    pub body: &'static str,
    pub position: (u16, u16),
}

pub(crate) const NOTICES: [StandingNotice; 2] = [
    StandingNotice {
        title: "공용 도로를 막지 마시오",
        body: "앨더마크 경비대에서 알린다.\n\n\
마을과 던전을 잇는 길은 모두의 것이다. 길 위와 길가에 세운 목책은 순찰대가 철거한다.\n\n\
영지 안이라도 길을 막은 것은 예외가 없다. 길에서 떨어진 목책은 건드리지 않는다.\n\n\
처음 철거된 목책은 내가 보관한다. 값의 절반을 내면 돌려준다. 내 초소로 오라.\n\n\
두 번째부터는 돌려주지 않는다.\n\n\
지금 길 위에 목책이 있는 이는 게임 시간으로 사흘 안에 치워라. 그 뒤부터 순찰대가 치운다.\n\n\
순찰은 낮과 밤 모두 돈다. 대원에게 따져도 소용없다. 할 말이 있으면 나에게 하라.\n\n\
— 경비대장 Karl",
        position: (200, 250),
    },
    StandingNotice {
        title: "Keep the public road open",
        body: "By order of the Aldermark Guard.\n\n\
The roads between the town and the dungeons belong to everyone. Fences standing on a road or at \
its edge will be pulled down by the patrol.\n\n\
An estate is no excuse for blocking the road. Fences set back from it are left alone.\n\n\
The first time, I hold what was taken. Pay half its price and it is yours again. Come to my \
post.\n\n\
The second time, nothing is returned.\n\n\
If your fence stands on a road today, clear it within three game days. After that the patrol \
clears it for you.\n\n\
The patrol walks day and night. Arguing with the guards changes nothing. Bring complaints to \
me.\n\n\
— Karl, Captain of the Guard",
        position: (400, 250),
    },
];

impl GameState {
    /// Renew the standing notices, at most once an hour.
    pub async fn tick_guard_notices(&self, auth: &AuthService) {
        let now = crate::auth::unix_now();
        if now - self.guard_notices_checked.load(Ordering::Relaxed) >= CHECK_EVERY_SECS {
            self.guard_notices_checked.store(now, Ordering::Relaxed);
            self.renew_guard_notices(auth).await;
        }
    }

    pub async fn renew_guard_notices(&self, auth: &AuthService) {
        let Some(captain) = npc_defs().npc_name_by_id(GUARD_CAPTAIN) else {
            return;
        };
        let now = self.current_total_game_seconds();
        let stale_at = now
            - (NOTICE_LIFETIME_DAYS - RENEW_DAYS_BEFORE_EXPIRY) * super::time::GAME_SECONDS_PER_DAY;
        let db = auth.clone();
        let renewed =
            auth_db(move || db.renew_standing_notices(BOARD, captain, &NOTICES, now, stale_at))
                .await;
        match renewed {
            Ok(0) => {}
            Ok(count) => tracing::info!(count, "guard notices posted"),
            Err(error) => tracing::warn!(%error, "Failed to renew guard notices"),
        }
    }
}
