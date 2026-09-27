use super::*;
use onlinerpg_shared::messages::{
    PlayerTradeSide, PlayerTradeSlot, PlayerTradeState, PLAYER_TRADE_IDLE_TTL,
    PLAYER_TRADE_REQUEST_TTL,
};
use tokio::time::Instant;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PlayerTradePrice {
    Buy { max_copper: i64 },
    Sell { min_copper: i64 },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlayerTradeTerms {
    pub item: String,
    pub quantity: u32,
    pub enchant: i32,
    pub price: PlayerTradePrice,
}

impl PlayerTradeTerms {
    fn limit(&self) -> i64 {
        match self.price {
            PlayerTradePrice::Buy { max_copper } => max_copper,
            PlayerTradePrice::Sell { min_copper } => min_copper,
        }
    }

    fn description(&self) -> String {
        let verb = match self.price {
            PlayerTradePrice::Buy { .. } => "buy; offering at most",
            PlayerTradePrice::Sell { .. } => "sell; accepting at least",
        };
        format!(
            "{} x{} (+{}), {verb} {} copper total",
            self.item,
            self.quantity,
            self.enchant,
            self.limit()
        )
    }

    fn matches_items(&self, side: &PlayerTradeSide) -> bool {
        side.items.iter().all(|item| {
            item.item_def_id == self.item && item.enchant == self.enchant && item.quantity > 0
        }) && side
            .items
            .iter()
            .map(|item| u64::from(item.quantity))
            .sum::<u64>()
            == u64::from(self.quantity)
    }
}

struct TradeIntent {
    partner_id: PlayerId,
    partner_name: String,
    terms: PlayerTradeTerms,
    items: Vec<PlayerTradeSlot>,
    deadline: Instant,
}

impl TradeIntent {
    fn copper(&self) -> i64 {
        match self.terms.price {
            PlayerTradePrice::Buy { max_copper } => max_copper,
            PlayerTradePrice::Sell { .. } => 0,
        }
    }

    fn own_offer_matches(&self, side: &PlayerTradeSide) -> bool {
        side.copper == self.copper()
            && side.items.len() == self.items.len()
            && side.items.iter().all(|item| {
                item.item_def_id == self.terms.item
                    && item.enchant == self.terms.enchant
                    && self.items.iter().any(|slot| {
                        slot.instance_id == item.instance_id && slot.quantity == item.quantity
                    })
            })
    }

    fn partner_offer_matches(&self, side: &PlayerTradeSide) -> bool {
        match self.terms.price {
            PlayerTradePrice::Buy { .. } => side.copper == 0 && self.terms.matches_items(side),
            PlayerTradePrice::Sell { min_copper } => {
                side.items.is_empty() && side.copper >= min_copper
            }
        }
    }
}

struct TradeRequest {
    id: PlayerId,
    name: String,
    expires_at: Instant,
}

#[derive(Clone, Copy)]
enum TradeStep {
    Request,
    Respond,
    Offer,
    Unlock,
    Lock(u32),
    Confirm(u32),
    Cancel,
}

#[derive(Default)]
pub(super) struct PlayerTradeAutomation {
    intent: Option<TradeIntent>,
    snapshot: Option<PlayerTradeState>,
    requests: Vec<TradeRequest>,
    in_flight: Option<TradeStep>,
    closing_deadline: Option<Instant>,
    last_problem: Option<String>,
}

fn offer_description(side: &PlayerTradeSide) -> String {
    let items = side
        .items
        .iter()
        .map(|item| {
            format!(
                "{} x{} (+{})",
                item.item_def_id, item.quantity, item.enchant
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{}], {} copper", items, side.copper)
}

impl SharedState {
    pub(crate) fn player_trade_in_progress(&self) -> bool {
        self.player_trade.intent.is_some() || self.player_trade.snapshot.is_some()
    }

    pub(crate) async fn start_player_trade(
        &mut self,
        player_name: &str,
        mut terms: PlayerTradeTerms,
    ) -> Result<(), String> {
        let me = self
            .self_player
            .as_ref()
            .ok_or("You are not in the game.")?;
        if me.is_official_npc || me.health == 0 {
            return Err(
                "Direct trades require a living player character, not an official NPC.".into(),
            );
        }
        if self.self_fishing {
            return Err("Stop fishing before starting a direct trade.".into());
        }
        if terms.quantity == 0 || terms.enchant < 0 || terms.limit() < 0 {
            return Err(
                "Use a positive quantity, nonnegative enchant and nonnegative total copper.".into(),
            );
        }
        let item = crate::item_defs::resolve_named(&crate::item_defs::all_ids(), &terms.item)
            .ok_or("Unknown item; use an item id from your inventory or the offer.")?
            .to_owned();
        terms.item = item;
        if crate::item_defs::get(&terms.item).is_some_and(|def| def.untradeable) {
            return Err("That item cannot be traded.".into());
        }
        let partner = self
            .nearby_players
            .values()
            .find(|player| player.name.eq_ignore_ascii_case(player_name))
            .ok_or("That player is not nearby. Move closer first.")?;
        if partner.id == me.id || partner.is_official_npc {
            return Err("Choose another player; official NPCs use shop trading.".into());
        }
        let partner_id = partner.id;
        let partner_name = partner.name.clone();
        if let Some(intent) = &self.player_trade.intent {
            if intent.partner_id == partner_id && intent.terms == terms {
                self.push_agent_event(
                    "[PlayerTrade] This trade intent is already running; wait for its result."
                        .into(),
                );
                return Ok(());
            }
            return Err("A trade intent is already running. Cancel it and wait for closure before changing terms.".into());
        }
        if self.player_trade.snapshot.is_some() {
            return Err("Wait for the current trade to close before starting another.".into());
        }
        let mut items = Vec::new();
        match terms.price {
            PlayerTradePrice::Buy { max_copper } => {
                if !self.self_gold.is_some_and(|gold| gold >= max_copper) {
                    return Err("Your known balance does not cover that total copper offer.".into());
                }
            }
            PlayerTradePrice::Sell { .. } => {
                let mut remaining = terms.quantity;
                for item in self.self_bag.iter().filter(|item| {
                    item.item_def_id == terms.item && item.enchant == terms.enchant && !item.locked
                }) {
                    let quantity = item.quantity.min(remaining);
                    if quantity > 0 {
                        items.push(PlayerTradeSlot {
                            instance_id: item.instance_id,
                            quantity,
                        });
                        remaining -= quantity;
                    }
                    if remaining == 0 {
                        break;
                    }
                }
                if remaining > 0 {
                    return Err("Not enough unlocked bag items with that exact enchant level; equipped items cannot be sold.".into());
                }
            }
        }
        self.cancel_follow();
        self.player_trade.intent = Some(TradeIntent {
            partner_id,
            partner_name: partner_name.clone(),
            terms: terms.clone(),
            items,
            deadline: Instant::now() + PLAYER_TRADE_REQUEST_TTL,
        });
        self.player_trade.in_flight = None;
        self.player_trade.last_problem = None;
        self.push_agent_event(format!("[PlayerTrade] With {partner_name}: {}. The client will handle the exchange; stay nearby.", terms.description()));
        self.drive_player_trade().await;
        Ok(())
    }

    pub(crate) async fn cancel_player_trade(&mut self) {
        let requests = std::mem::take(&mut self.player_trade.requests);
        self.player_trade.intent = None;
        self.player_trade.in_flight = None;
        self.player_trade.last_problem = None;
        for request in requests {
            if request.expires_at > Instant::now() {
                let _ = self
                    .send_command(ClientMessage::PlayerTradeRespond {
                        requester_id: request.id,
                        accept: false,
                    })
                    .await;
            }
        }
        self.drive_player_trade().await;
        self.push_agent_event("[PlayerTrade] Trade intent withdrawn and pending invitations declined. Any open exchange is being cancelled; wait for the server result.".into());
    }

    pub(super) fn apply_player_trade_event(&mut self, msg: &ServerMessage) -> bool {
        match msg {
            ServerMessage::PlayerTradeRequested {
                requester_id,
                requester_name,
            } => {
                let now = Instant::now();
                let trade = &mut self.player_trade;
                trade
                    .requests
                    .retain(|request| request.expires_at > now && request.id != *requester_id);
                if trade.requests.len() >= 5 {
                    trade.requests.remove(0);
                }
                trade.requests.push(TradeRequest {
                    id: *requester_id,
                    name: requester_name.clone(),
                    expires_at: now + PLAYER_TRADE_REQUEST_TTL,
                });
                if trade.snapshot.is_none()
                    && trade
                        .intent
                        .as_ref()
                        .is_some_and(|intent| intent.partner_id == *requester_id)
                {
                    trade.in_flight = None;
                } else {
                    self.push_ambient_event(format!("[PlayerTradeRequested] {requester_name} wants to trade. Choose buy_from_player or sell_to_player with explicit terms, or cancel_player_trade to decline."));
                    self.wake(EventUrgency::Urgent);
                }
            }
            ServerMessage::PlayerTradeRequestResult {
                target_name,
                accepted,
                message,
            } => {
                if !accepted
                    && self.player_trade.snapshot.is_none()
                    && matches!(self.player_trade.in_flight, Some(TradeStep::Request))
                    && self
                        .player_trade
                        .intent
                        .as_ref()
                        .is_some_and(|intent| intent.partner_name == *target_name)
                {
                    self.player_trade.intent = None;
                    self.player_trade.in_flight = None;
                    self.push_ambient_event(format!("[PlayerTradeFailed] {message}"));
                }
            }
            ServerMessage::PlayerTradeUpdate { state } => {
                let trade = &mut self.player_trade;
                if let Some(intent) = &mut trade.intent {
                    if Some(state.you.player_id) != self.self_player_id
                        || state.them.player_id != intent.partner_id
                    {
                        trade.intent = None;
                        trade.in_flight = None;
                    } else {
                        if trade.snapshot.is_none() {
                            intent.deadline = Instant::now() + PLAYER_TRADE_IDLE_TTL;
                        }
                        let acknowledged = match trade.in_flight {
                            Some(TradeStep::Request | TradeStep::Respond) => true,
                            Some(TradeStep::Offer) => intent.own_offer_matches(&state.you),
                            Some(TradeStep::Unlock) => !state.you.locked,
                            Some(TradeStep::Lock(revision)) => {
                                state.you.locked || state.revision != revision
                            }
                            Some(TradeStep::Confirm(revision)) => {
                                state.you.confirmed || state.revision != revision
                            }
                            _ => false,
                        };
                        if acknowledged {
                            trade.in_flight = None;
                        }
                    }
                } else {
                    trade.in_flight = None;
                }
                trade.snapshot = Some(state.clone());
            }
            ServerMessage::PlayerTradeEnded { completed, message } => {
                self.player_trade = Default::default();
                let result = if *completed {
                    "PlayerTradeCompleted"
                } else {
                    "PlayerTradeEnded"
                };
                self.push_ambient_event(format!("[{result}] {message}"));
                self.wake(EventUrgency::Urgent);
            }
            ServerMessage::PlayerTradeError { message } => {
                self.player_trade.intent = None;
                self.player_trade.in_flight = None;
                self.push_ambient_event(format!("[PlayerTradeFailed] {message} The intent has stopped; the client will close any open exchange."));
                self.wake(EventUrgency::Urgent);
            }
            _ => return false,
        }
        true
    }

    pub(crate) async fn drive_player_trade(&mut self) {
        let now = Instant::now();
        if self
            .player_trade
            .closing_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.player_trade = Default::default();
            self.push_ambient_event("[PlayerTradeFailed] No closing result arrived. The local intent was discarded; check the server result before assuming a trade completed.".into());
            return;
        }
        self.player_trade
            .requests
            .retain(|request| request.expires_at > now);
        if self
            .player_trade
            .intent
            .as_ref()
            .is_some_and(|intent| now >= intent.deadline)
        {
            self.player_trade.intent = None;
            self.player_trade.in_flight = None;
            self.push_ambient_event(
                "[PlayerTradeFailed] The trade intent timed out and was withdrawn.".into(),
            );
        }
        if self.player_trade.in_flight.is_some() {
            return;
        }
        let Some((step, command)) = self.next_player_trade_command() else {
            return;
        };
        if matches!(step, TradeStep::Cancel) {
            self.player_trade
                .closing_deadline
                .get_or_insert(now + PLAYER_TRADE_REQUEST_TTL);
        }
        self.player_trade.in_flight = Some(step);
        if let Err(error) = self.send_background_command(command).await {
            self.player_trade = Default::default();
            self.push_ambient_event(format!(
                "[PlayerTradeFailed] Could not send the trade command: {error}"
            ));
        }
    }

    fn next_player_trade_command(&mut self) -> Option<(TradeStep, ClientMessage)> {
        let trade = &mut self.player_trade;
        let Some(intent) = &trade.intent else {
            return trade
                .snapshot
                .as_ref()
                .map(|_| (TradeStep::Cancel, ClientMessage::PlayerTradeCancel));
        };
        let Some(state) = &trade.snapshot else {
            let (step, command) = if let Some(index) = trade
                .requests
                .iter()
                .position(|request| request.id == intent.partner_id)
            {
                trade.requests.remove(index);
                (
                    TradeStep::Respond,
                    ClientMessage::PlayerTradeRespond {
                        requester_id: intent.partner_id,
                        accept: true,
                    },
                )
            } else {
                (
                    TradeStep::Request,
                    ClientMessage::PlayerTradeRequest {
                        target_name: intent.partner_name.clone(),
                    },
                )
            };
            return Some((step, command));
        };
        if !intent.own_offer_matches(&state.you) {
            return Some(if state.you.locked {
                (TradeStep::Unlock, ClientMessage::PlayerTradeUnlock)
            } else {
                (
                    TradeStep::Offer,
                    ClientMessage::PlayerTradeSetOffer {
                        items: intent.items.clone(),
                        copper: intent.copper(),
                    },
                )
            });
        }
        if !intent.partner_offer_matches(&state.them) {
            let unlock = state
                .you
                .locked
                .then_some((TradeStep::Unlock, ClientMessage::PlayerTradeUnlock));
            if state.them.locked || !state.them.items.is_empty() || state.them.copper != 0 {
                let problem = format!("[PlayerTradeOffer] {} offers {}. This does not meet your terms: {}. Negotiate in chat, wait for a corrected offer, or cancel before issuing new terms.", intent.partner_name, offer_description(&state.them), intent.terms.description());
                if trade.last_problem.as_ref() != Some(&problem) {
                    trade.last_problem = Some(problem.clone());
                    self.push_ambient_event(problem);
                }
            }
            return unlock;
        }
        trade.last_problem = None;
        if !state.you.locked {
            Some((
                TradeStep::Lock(state.revision),
                ClientMessage::PlayerTradeLock {
                    revision: state.revision,
                },
            ))
        } else if state.them.locked && !state.you.confirmed {
            Some((
                TradeStep::Confirm(state.revision),
                ClientMessage::PlayerTradeConfirm {
                    revision: state.revision,
                },
            ))
        } else {
            None
        }
    }

    pub(super) fn format_player_trade(&self) -> Option<String> {
        let trade = &self.player_trade;
        let mut lines = Vec::new();
        for request in trade
            .requests
            .iter()
            .filter(|request| request.expires_at > Instant::now())
        {
            lines.push(format!("Trade invitation from {}: choose buy_from_player/sell_to_player with terms, or cancel_player_trade to decline.", request.name));
        }
        if let Some(intent) = &trade.intent {
            lines.push(format!("Player trade with {}: {}. Exchange steps are automatic. Stay nearby; use cancel_player_trade before changing terms.", intent.partner_name, intent.terms.description()));
        }
        if let Some(state) = &trade.snapshot {
            lines.push(format!(
                "Trade table: you offer {}; {} offers {}. Your agreement: {}; theirs: {}.",
                offer_description(&state.you),
                state.them.name,
                offer_description(&state.them),
                state.you.confirmed,
                state.them.confirmed
            ));
            if trade.intent.is_none() {
                lines.push("Closing this exchange; wait for the server result.".into());
            }
        }
        (!lines.is_empty()).then(|| lines.join("\n"))
    }
}
