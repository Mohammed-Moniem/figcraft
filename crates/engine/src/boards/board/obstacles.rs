//! A complete, short-lived obstacle snapshot. Never cache a detour across document revisions.
use std::collections::{BTreeMap, BTreeSet};

use super::{Affine, BoardItem, Connector, Document, NodeId, Point, Rect, Routing};
use figcraft_doc::{Node, RoutingBudget};
use figcraft_geom::transform_rect_bbox;

const NODES: usize = 16_384;
const DEPTH: usize = 64;
const PAGES: usize = 16;
const PER_PAGE: usize = 256;
const TOTAL: usize = 1024;

struct Obstacle {
    bounds: Rect,
    chain: Vec<NodeId>,
}

/// One query/settlement owns its snapshot and work budget. An incomplete snapshot is never used.
pub(super) struct Context {
    pages: BTreeMap<NodeId, Vec<Obstacle>>,
    complete: bool,
    budget: RoutingBudget,
}

fn chain<'a>(doc: &'a Document, id: NodeId, budget: &mut RoutingBudget) -> Option<Vec<&'a Node>> {
    let mut nodes = Vec::new();
    let mut current = Some(id);
    while let Some(id) = current {
        if nodes.len() >= DEPTH || !budget.charge(1) || nodes.iter().any(|n: &&Node| n.id == id) {
            return None;
        }
        let node = doc.node(id)?;
        nodes.push(node);
        if node.is_page() {
            return Some(nodes);
        }
        current = node.parent;
    }
    None
}

fn finite(r: Rect) -> bool {
    [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite()) && r.x0 < r.x1 && r.y0 < r.y1
}

/// Conservative visible bounds, including every ancestor's transform, visibility and clip box.
fn obstacle(nodes: &[&Node]) -> Option<Option<Rect>> {
    let mut world = Affine::IDENTITY;
    let mut clip: Option<Rect> = None;
    for (i, n) in nodes.iter().rev().enumerate() {
        if !n.visible || !n.opacity.is_finite() {
            return if n.opacity.is_finite() { Some(None) } else { None };
        }
        if n.opacity <= 0.0 || (i + 1 < nodes.len() && n.hides_contents()) {
            return Some(None);
        }
        if n.is_page() {
            continue;
        }
        world *= n.transform;
        let bounds = transform_rect_bbox(world, n.local_rect());
        if !finite(bounds) {
            return None;
        }
        if n.clip && i + 1 < nodes.len() {
            clip = Some(clip.map_or(bounds, |c| c.intersect(bounds)));
        }
    }
    let own = transform_rect_bbox(world, nodes.first()?.local_rect());
    let visible = clip.map_or(own, |c| c.intersect(own));
    Some(finite(visible).then_some(visible))
}

impl Context {
    pub(super) fn new(doc: &Document) -> Self {
        let mut out = Self { pages: BTreeMap::new(), complete: true, budget: RoutingBudget::default() };
        let mut total = 0;
        for (index, n) in doc.nodes().enumerate() {
            if index >= NODES || !out.budget.charge(1) {
                out.complete = false;
                break;
            }
            if !matches!(n.board, Some(BoardItem::Shape { .. } | BoardItem::Sticky { .. })) {
                continue;
            }
            let Some(nodes) = chain(doc, n.id, &mut out.budget) else {
                out.complete = false;
                break;
            };
            let Some(page) = nodes.last().map(|n| n.id) else {
                out.complete = false;
                break;
            };
            let Some(bounds) = obstacle(&nodes) else {
                out.complete = false;
                break;
            };
            let Some(bounds) = bounds else {
                continue;
            };
            if (!out.pages.contains_key(&page) && out.pages.len() >= PAGES) || total >= TOTAL {
                out.complete = false;
                break;
            }
            let list = out.pages.entry(page).or_default();
            if list.len() >= PER_PAGE {
                out.complete = false;
                break;
            }
            list.push(Obstacle { bounds, chain: nodes.iter().map(|n| n.id).collect() });
            total += 1;
        }
        out
    }

    pub(super) fn route(&mut self, doc: &Document, c: &Connector) -> Option<Vec<Point>> {
        let (from, to) = super::derived::route_ends(doc, c)?;
        let legacy = || figcraft_doc::route_along(from, to, c.routing, c.path.as_ref());
        if !automatic(c) || !self.complete {
            return legacy();
        }
        let mut ends = Vec::new();
        let mut page = None;
        for id in [c.from.node(), c.to.node()].into_iter().flatten() {
            let Some(nodes) = chain(doc, id, &mut self.budget) else {
                return legacy();
            };
            let Some(p) = nodes.last().map(|n| n.id) else {
                return legacy();
            };
            if page.is_some_and(|old| old != p) {
                return legacy();
            }
            page = Some(p);
            ends.push(nodes.iter().map(|n| n.id).collect::<Vec<_>>());
        }
        let Some(list) = page.and_then(|p| self.pages.get(&p)) else {
            return legacy();
        };
        let mut rects = Vec::new();
        for obstacle in list {
            if !self.budget.charge(1) {
                return legacy();
            }
            let Some(owner) = obstacle.chain.first() else {
                return legacy();
            };
            let related = ends.iter().any(|end| end.contains(owner) || end.first().is_some_and(|e| obstacle.chain.contains(e)));
            if !related {
                rects.push(obstacle.bounds);
            }
        }
        figcraft_doc::route_avoiding(from, to, &rects, &mut self.budget)
    }

    /// Only an automatic elbow can be invalidated by an unrelated obstacle edit.
    pub(super) fn refresh(&mut self, doc: &Document, id: NodeId, c: &Connector, affected: &Affected) -> bool {
        if !automatic(c) {
            return false;
        }
        match affected {
            Affected::All => true,
            Affected::Pages(pages) if pages.is_empty() => false,
            Affected::Pages(pages) => chain(doc, id, &mut self.budget).and_then(|v| v.last().map(|n| n.id)).is_none_or(|p| pages.contains(&p)),
        }
    }
}

fn automatic(c: &Connector) -> bool {
    c.routing == Routing::Elbow && c.path.is_none()
}

fn changes_obstacles(now: Option<&Node>, old: Option<&Node>) -> bool {
    let is_obstacle = |n: &Node| matches!(n.board, Some(BoardItem::Shape { .. } | BoardItem::Sticky { .. }));
    let relevant = |n: &Node| is_obstacle(n) || n.kind.is_container();
    match (now, old) {
        (Some(a), Some(b)) => {
            (relevant(a) || relevant(b))
                && (is_obstacle(a) != is_obstacle(b)
                    || a.parent != b.parent
                    || a.transform != b.transform
                    || a.width != b.width
                    || a.height != b.height
                    || a.visible != b.visible
                    || a.opacity != b.opacity
                    || a.clip != b.clip
                    || a.hides_contents() != b.hides_contents())
        }
        (Some(n), None) | (None, Some(n)) => relevant(n),
        _ => false,
    }
}

pub(super) enum Affected {
    All,
    Pages(BTreeSet<NodeId>),
}

impl Affected {
    pub(super) fn new(doc: &Document, before: &Document, touched: &BTreeSet<NodeId>, context: &mut Context) -> Self {
        let mut pages = BTreeSet::new();
        for &id in touched {
            if !context.budget.charge(1) {
                return Self::All;
            }
            if !changes_obstacles(doc.node(id), before.node(id)) {
                continue;
            }
            for source in [doc, before] {
                let Some(node) = source.node(id) else {
                    continue;
                };
                if matches!(node.board, Some(BoardItem::Connector(_) | BoardItem::Label { .. })) {
                    continue;
                }
                let Some(nodes) = chain(source, id, &mut context.budget) else {
                    return Self::All;
                };
                let Some(page) = nodes.last().map(|n| n.id) else {
                    return Self::All;
                };
                pages.insert(page);
                if pages.len() > PAGES {
                    return Self::All;
                }
            }
        }
        Self::Pages(pages)
    }
}
