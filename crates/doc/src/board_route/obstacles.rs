//! Bounded orthogonal avoidance for automatic native Board elbows.
use std::cmp::Ordering;
use std::collections::BinaryHeap;

use super::{END_GAP, ROOM, RouteEnd, attach, route_points, tidy};
use crate::{Routing, Side};
use figcraft_geom::{Point, Rect};

const WORK: usize = 262_144;
const SEARCHES: usize = 64;
const OBSTACLES: usize = 256;
const ACTIVE: usize = 32;
const GRID: usize = 4096;
const QUEUE: usize = 8192;
const REPLANS: usize = 4;
const POINTS: usize = 64;
const CLEARANCE: f64 = 8.0;
const STUB: f64 = 8.0;

/// One top-level query/normalization's finite work allowance. Share it across derived routes.
pub struct RoutingBudget {
    remaining: usize,
    searches: usize,
}

impl Default for RoutingBudget {
    fn default() -> Self {
        Self { remaining: WORK, searches: 0 }
    }
}

impl RoutingBudget {
    /// Charge before data-dependent work. Exhaustion always chooses the legacy route.
    pub fn charge(&mut self, amount: usize) -> bool {
        if amount > self.remaining {
            self.remaining = 0;
            false
        } else {
            self.remaining -= amount;
            true
        }
    }

    fn begin(&mut self) -> bool {
        if self.searches >= SEARCHES || !self.charge(1) {
            return false;
        }
        self.searches += 1;
        true
    }
}

fn valid(r: Rect) -> bool {
    [r.x0, r.y0, r.x1, r.y1].iter().all(|n| n.is_finite()) && r.x0 < r.x1 && r.y0 < r.y1
}

fn expanded(r: Rect, by: f64) -> Option<Rect> {
    let r = Rect::new(r.x0 - by, r.y0 - by, r.x1 + by, r.y1 + by);
    valid(r).then_some(r)
}

fn inside(p: Point, r: Rect) -> bool {
    p.x > r.x0 && p.x < r.x1 && p.y > r.y0 && p.y < r.y1
}

fn crosses(a: Point, b: Point, r: Rect) -> bool {
    if a.x == b.x {
        a.x > r.x0 && a.x < r.x1 && a.y.min(b.y) < r.y1 && a.y.max(b.y) > r.y0
    } else if a.y == b.y {
        a.y > r.y0 && a.y < r.y1 && a.x.min(b.x) < r.x1 && a.x.max(b.x) > r.x0
    } else {
        true
    }
}

fn clear(a: Point, b: Point, rects: &[Rect], budget: &mut RoutingBudget) -> Option<bool> {
    for &r in rects {
        if !budget.charge(1) {
            return None;
        }
        if crosses(a, b, r) {
            return Some(false);
        }
    }
    Some(true)
}

fn blockers(points: &[Point], rects: &[Rect], budget: &mut RoutingBudget) -> Option<Vec<usize>> {
    let mut found = Vec::new();
    for (i, &r) in rects.iter().enumerate() {
        for pair in points.windows(2) {
            if !budget.charge(1) {
                return None;
            }
            if crosses(pair[0], pair[1], r) {
                found.push(i);
                break;
            }
        }
    }
    Some(found)
}

/// Automatic elbow geometry with centerline clearance from a complete same-page obstacle set.
/// Manual paths and other styles must use route_along. Invalid/incomplete/over-budget work
/// returns the freshly computed endpoint-only route, never a partly validated detour.
pub fn route_avoiding(from: RouteEnd, to: RouteEnd, obstacles: &[Rect], budget: &mut RoutingBudget) -> Option<Vec<Point>> {
    let legacy = route_points(from, to, Routing::Elbow)?;
    let Some(candidate) = avoid(from, to, &legacy, obstacles, budget) else {
        return Some(legacy);
    };
    Some(candidate)
}

fn avoid(from: RouteEnd, to: RouteEnd, legacy: &[Point], obstacles: &[Rect], budget: &mut RoutingBudget) -> Option<Vec<Point>> {
    if obstacles.is_empty() || obstacles.len() > OBSTACLES || !budget.begin() {
        return None;
    }
    let rects: Option<Vec<_>> = obstacles.iter().map(|&r| valid(r).then(|| expanded(r, CLEARANCE)).flatten()).collect();
    let rects = rects?;
    let mut active = blockers(legacy, &rects, budget)?;
    if active.is_empty() || active.len() > ACTIVE {
        return None;
    }
    let (p0, s0) = attach(from, to.centre());
    let (p1, s1) = attach(to, from.centre());
    let a = p0 + s0.map_or(figcraft_geom::Vec2::ZERO, |s| s.normal() * STUB);
    let b = p1 + s1.map_or(figcraft_geom::Vec2::ZERO, |s| s.normal() * STUB);
    if !clear(p0, a, &rects, budget)? || !clear(b, p1, &rects, budget)? {
        return None;
    }
    let ends: Option<Vec<_>> = [from, to]
        .into_iter()
        .filter_map(|e| match e {
            RouteEnd::Box(r, _) => Some(expanded(r, END_GAP)),
            _ => None,
        })
        .collect();
    let ends = ends?;
    for _ in 0..REPLANS {
        let mut selected: Vec<_> = active.iter().filter_map(|&i| rects.get(i).copied()).collect();
        selected.extend_from_slice(&ends);
        let grid = Grid::new(a, b, s0, s1, &selected, budget)?;
        let mut points = vec![p0];
        points.extend(search(&grid, &selected, budget)?);
        points.push(p1);
        let mut points = tidy(points);
        if points.len() > POINTS || points.len() < 2 {
            return None;
        }
        *points.first_mut()? = p0;
        *points.last_mut()? = p1;
        let missing = blockers(&points, &rects, budget)?;
        if missing.is_empty() {
            return Some(points);
        }
        let old = active.len();
        for i in missing {
            if !active.contains(&i) {
                active.push(i);
            }
        }
        if active.len() == old || active.len() > ACTIVE {
            return None;
        }
    }
    None
}

struct Grid {
    xs: Vec<f64>,
    ys: Vec<f64>,
    blocked: Vec<bool>,
    start: usize,
    goal: usize,
    first: Option<Side>,
    last: Option<Side>,
}

impl Grid {
    fn new(a: Point, b: Point, first: Option<Side>, last: Option<Side>, rects: &[Rect], budget: &mut RoutingBudget) -> Option<Self> {
        let (mut xs, mut ys) = (vec![a.x, b.x], vec![a.y, b.y]);
        for r in rects {
            xs.extend([r.x0, r.x1]);
            ys.extend([r.y0, r.y1]);
        }
        let sort = |v: &mut Vec<f64>| {
            v.sort_by(f64::total_cmp);
            v.dedup_by(|a, b| *a == *b);
        };
        sort(&mut xs);
        sort(&mut ys);
        let (x0, x1, y0, y1) = (*xs.first()?, *xs.last()?, *ys.first()?, *ys.last()?);
        xs.extend([x0 - ROOM, x1 + ROOM]);
        ys.extend([y0 - ROOM, y1 + ROOM]);
        sort(&mut xs);
        sort(&mut ys);
        if !xs.iter().chain(&ys).all(|v| v.is_finite()) {
            return None;
        }
        let count = xs.len().checked_mul(ys.len()).filter(|&n| n > 0 && n <= GRID)?;
        let index = |p: Point| ys.iter().position(|&v| v == p.y)?.checked_mul(xs.len())?.checked_add(xs.iter().position(|&v| v == p.x)?);
        let (start, goal) = (index(a)?, index(b)?);
        let mut out = Self { xs, ys, blocked: vec![false; count], start, goal, first, last };
        for i in 0..count {
            let p = out.point(i)?;
            for &r in rects {
                if !budget.charge(1) {
                    return None;
                }
                if inside(p, r) {
                    *out.blocked.get_mut(i)? = true;
                    break;
                }
            }
        }
        if *out.blocked.get(start)? || *out.blocked.get(goal)? {
            return None;
        }
        Some(out)
    }

    fn point(&self, index: usize) -> Option<Point> {
        if self.xs.is_empty() {
            return None;
        }
        Some(Point::new(*self.xs.get(index % self.xs.len())?, *self.ys.get(index / self.xs.len())?))
    }

    fn neighbors(&self, i: usize) -> [Option<usize>; 4] {
        let w = self.xs.len();
        let x = if w > 0 { i % w } else { 0 };
        [
            i.checked_sub(1).filter(|_| x > 0),
            i.checked_add(1).filter(|_| x + 1 < w),
            i.checked_sub(w),
            i.checked_add(w).filter(|&j| j < self.blocked.len()),
        ]
    }

    fn permitted(&self, from: usize, to: usize) -> Option<bool> {
        let d = self.point(to)? - self.point(from)?;
        let dot = |s: Side| {
            let n = s.normal();
            d.x * n.x + d.y * n.y
        };
        Some(!(from == self.start && self.first.is_some_and(|s| dot(s) < 0.0) || to == self.goal && self.last.is_some_and(|s| dot(s) > 0.0)))
    }
}

#[derive(Clone, Copy)]
struct Entry {
    estimate: f64,
    cost: f64,
    state: usize,
}
impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.estimate.to_bits() == other.estimate.to_bits() && self.cost.to_bits() == other.cost.to_bits() && self.state == other.state
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other.estimate.total_cmp(&self.estimate).then_with(|| other.cost.total_cmp(&self.cost)).then_with(|| other.state.cmp(&self.state))
    }
}

fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}
fn direction(s: Side) -> usize {
    let n = s.normal();
    usize::from(n.y != 0.0)
}

struct Search {
    queue: BinaryHeap<Entry>,
    costs: Vec<f64>,
    previous: Vec<Option<usize>>,
}

impl Search {
    fn push(&mut self, state: usize, cost: f64, estimate: f64, previous: Option<usize>) -> Option<()> {
        if !cost.is_finite() || !estimate.is_finite() || self.queue.len() >= QUEUE {
            return None;
        }
        *self.costs.get_mut(state)? = cost;
        *self.previous.get_mut(state)? = previous;
        self.queue.push(Entry { estimate, cost, state });
        Some(())
    }

    fn step(&mut self, grid: &Grid, entry: Entry, next: usize, rects: &[Rect], budget: &mut RoutingBudget) -> Option<()> {
        if *grid.blocked.get(next)? || !grid.permitted(entry.state / 2, next)? {
            return Some(());
        }
        let (p, q) = (grid.point(entry.state / 2)?, grid.point(next)?);
        if !clear(p, q, rects, budget)? {
            return Some(());
        }
        let dir = usize::from(p.x == q.x);
        let state = next.checked_mul(2)?.checked_add(dir)?;
        let bend = if dir != entry.state % 2 { ROOM } else { 0.0 };
        let last = if next == grid.goal && grid.last.is_some_and(|s| direction(s) != dir) { ROOM } else { 0.0 };
        let cost = entry.cost + distance(p, q) + bend + last;
        if cost < *self.costs.get(state)? {
            self.push(state, cost, cost + distance(q, grid.point(grid.goal)?), Some(entry.state))?;
        }
        Some(())
    }
}

fn search(grid: &Grid, rects: &[Rect], budget: &mut RoutingBudget) -> Option<Vec<Point>> {
    let states = grid.blocked.len().checked_mul(2).filter(|&n| n <= 8192)?;
    let mut run = Search { queue: BinaryHeap::new(), costs: vec![f64::INFINITY; states], previous: vec![None; states] };
    for dir in 0..2 {
        if grid.first.is_none_or(|s| direction(s) == dir) {
            run.push(grid.start.checked_mul(2)?.checked_add(dir)?, 0.0, distance(grid.point(grid.start)?, grid.point(grid.goal)?), None)?;
        }
    }
    while let Some(entry) = run.queue.pop() {
        if !budget.charge(1) {
            return None;
        }
        if entry.cost != *run.costs.get(entry.state)? {
            continue;
        }
        if entry.state / 2 == grid.goal {
            return reconstruct(grid, &run, entry.state);
        }
        for next in grid.neighbors(entry.state / 2).into_iter().flatten() {
            if !budget.charge(1) {
                return None;
            }
            run.step(grid, entry, next, rects, budget)?;
        }
    }
    None
}

fn reconstruct(grid: &Grid, run: &Search, mut state: usize) -> Option<Vec<Point>> {
    let mut points = Vec::new();
    for _ in 0..run.previous.len() {
        points.push(grid.point(state / 2)?);
        match *run.previous.get(state)? {
            Some(p) => state = p,
            None => {
                points.reverse();
                return Some(points);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_and_report_detours_preserve_anchors_and_clear_every_obstacle() {
        for (from, to, obstacle) in [
            (Rect::new(0.0, 0.0, 100.0, 100.0), Rect::new(400.0, 0.0, 500.0, 100.0), Rect::new(200.0, 10.0, 280.0, 90.0)),
            (Rect::new(0.0, 0.0, 320.0, 120.0), Rect::new(520.0, 420.0, 800.0, 540.0), Rect::new(520.0, 180.0, 800.0, 300.0)),
        ] {
            let (a, b) = (RouteEnd::Box(from, Side::Auto), RouteEnd::Box(to, Side::Auto));
            let legacy = route_points(a, b, Routing::Elbow).unwrap();
            let mut budget = RoutingBudget::default();
            let routed = route_avoiding(a, b, &[obstacle], &mut budget).unwrap();
            assert_ne!(routed, legacy);
            assert_eq!(routed.first(), legacy.first());
            assert_eq!(routed.last(), legacy.last());
            assert!(clear_segments(&routed, obstacle));
            assert!(budget.remaining > 0);
        }
    }

    fn clear_segments(pts: &[Point], r: Rect) -> bool {
        pts.windows(2).all(|w| !crosses(w[0], w[1], r))
    }

    #[test]
    fn exhaustion_invalid_context_and_blocked_stub_use_exact_legacy_fallback() {
        let (a, b) = (RouteEnd::Box(Rect::new(0.0, 0.0, 100.0, 100.0), Side::Right), RouteEnd::Box(Rect::new(400.0, 0.0, 500.0, 100.0), Side::Left));
        let old = route_points(a, b, Routing::Elbow).unwrap();
        let obstacle = Rect::new(200.0, 10.0, 280.0, 90.0);
        let mut empty = RoutingBudget { remaining: 0, searches: 0 };
        assert_eq!(route_avoiding(a, b, &[obstacle], &mut empty).unwrap(), old);
        for rects in [vec![Rect::new(f64::NAN, 0.0, 1.0, 1.0)], vec![obstacle; 257], vec![Rect::new(100.0, 40.0, 120.0, 60.0)]] {
            assert_eq!(route_avoiding(a, b, &rects, &mut RoutingBudget::default()).unwrap(), old);
        }
        let mut searches = RoutingBudget { remaining: WORK, searches: SEARCHES };
        assert_eq!(route_avoiding(a, b, &[obstacle], &mut searches).unwrap(), old);
    }
}
