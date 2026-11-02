use super::*;
use crate::state::WorldCache;
use onlinerpg_shared::fence::{Fence, FenceAxis, FenceEdge};
use onlinerpg_shared::pathfinding::{PathTermination, DEFAULT_MAX_NODES};
use onlinerpg_shared::PlayerId;

fn edges(x0: i32, z0: i32, x1: i32, z1: i32, gaps: &[(i32, i32)]) -> Vec<FenceEdge> {
    (x0..x1)
        .flat_map(|x| [(x, z0, FenceAxis::X), (x, z1, FenceAxis::X)])
        .chain((z0..z1).flat_map(|z| [(x0, z, FenceAxis::Z), (x1, z, FenceAxis::Z)]))
        .filter(|&(x, z, _)| !gaps.contains(&(x, z)))
        .map(|(x, z, axis)| FenceEdge { x, z, axis })
        .collect()
}

fn cache_of<'a>(edges: impl Iterator<Item = &'a FenceEdge>) -> PassabilityCache {
    let fences: Vec<_> = edges
        .map(|&edge| Fence {
            edge,
            y: 0.0,
            owner_id: 0,
        })
        .collect();
    let mut world = WorldCache::new();
    world.update_fences(PlayerId::from(1), &fences, &[]);
    world.passability_cache().clone()
}

fn seen_from(all: &[FenceEdge], (x, z): (f32, f32)) -> PassabilityCache {
    cache_of(
        all.iter()
            .filter(|e| (e.x as f32 - x).hypot(e.z as f32 - z) <= EVENT_DELIVERY_RADIUS),
    )
}

/// Walks the way the client does, with the server's own per-request search.
fn arrives(
    all: &[FenceEdge],
    known: impl Fn((f32, f32)) -> PassabilityCache,
    from: (f32, f32),
    to: (f32, f32),
    planned: bool,
) -> bool {
    let server = cache_of(all.iter());
    let mut route = LongRoute::default();
    let mut at = from;
    for _ in 0..80 {
        if (to.0 - at.0).hypot(to.1 - at.1) < 1.0 {
            return true;
        }
        let client = known(at);
        let via_route = planned && toward(at, to) != to;
        let leg = if via_route {
            let Some(leg) = route.leg(&client, at, to) else {
                return false;
            };
            leg
        } else {
            toward(at, to)
        };
        let path = pathfinding::find_and_smooth_path(
            at.0,
            at.1,
            0,
            leg.0,
            leg.1,
            0,
            &server,
            DEFAULT_MAX_NODES,
        );
        let end = path.waypoints.last().map_or(at, |w| (w.x, w.z));
        if path.termination != PathTermination::Reached {
            if !planned && (end.0 - at.0).hypot(end.1 - at.1) < 1.5 {
                return false;
            }
            if via_route {
                route.leg_failed(&client, leg);
            }
        }
        at = end;
    }
    false
}

#[test]
fn open_ground_takes_full_legs_along_the_line() {
    let mut route = LongRoute::default();
    let leg = route
        .leg(&PassabilityCache::new(), (0.5, 0.5), (300.5, 0.5))
        .unwrap();
    assert!(leg.0 > 40.0 && leg.0 <= 48.5, "{leg:?}");
    assert!(leg.1.abs() < 8.0, "{leg:?}");
}

#[test]
fn a_closed_estate_on_the_line_is_walked_around() {
    let all = edges(40, -60, 140, 60, &[]);
    let (from, to) = ((0.5, 0.5), (180.5, 0.5));
    assert!(!arrives(&all, |_| cache_of(all.iter()), from, to, false));
    assert!(arrives(&all, |_| cache_of(all.iter()), from, to, true));
}

#[test]
fn fences_seen_only_up_close_are_still_walked_around() {
    let all = edges(40, -60, 140, 60, &[]);
    let (from, to) = ((0.5, 0.5), (180.5, 0.5));
    assert!(!arrives(&all, |at| seen_from(&all, at), from, to, false));
    assert!(arrives(&all, |at| seen_from(&all, at), from, to, true));
}

#[test]
fn a_pen_open_away_from_us_is_left_by_its_gate() {
    let gate: Vec<_> = (100..104).map(|z| (60, z)).collect();
    let all = edges(-60, -60, 60, 120, &gate);
    let (from, to) = ((0.5, 0.5), (0.5, 300.5));
    assert!(!arrives(&all, |at| seen_from(&all, at), from, to, false));
    assert!(arrives(&all, |at| seen_from(&all, at), from, to, true));
}

#[test]
fn a_goal_walled_in_has_no_route() {
    let all = edges(190, -10, 210, 10, &[]);
    let mut route = LongRoute::default();
    assert_eq!(
        route.leg(&cache_of(all.iter()), (0.5, 0.5), (200.5, 0.5)),
        None
    );
}

#[test]
fn a_failed_leg_is_not_offered_again() {
    let mut route = LongRoute::default();
    let cache = PassabilityCache::new();
    let first = route.leg(&cache, (0.5, 0.5), (300.5, 0.5)).unwrap();
    route.leg_failed(&cache, first);
    let second = route.leg(&cache, (0.5, 0.5), (300.5, 0.5)).unwrap();
    assert_ne!(sector_of(first), sector_of(second));
}
