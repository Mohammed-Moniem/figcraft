//! Whether a live Boolean result can expose the uniform radius stored on its own node.

use crate::{Document, NodeId, NodeKind};

/// Bound hostile/deep descendant graphs before extending the work list.
const MAX_CORNER_NODES: usize = 100_000;

impl Document {
    /// Kinds with radius controls, with Boolean eligibility resolved from their operands.
    /// A Boolean containing a rectangle can round its result; ellipse-only groups cannot.
    pub fn has_corner_radius(&self, id: NodeId) -> bool {
        let Some(node) = self.node(id) else { return false };
        if !matches!(node.kind, NodeKind::BooleanOp { .. }) {
            return node.kind.has_corner_radius();
        }
        if node.children.len() > MAX_CORNER_NODES {
            return false;
        }
        let mut pending = node.children.clone();
        let mut visited = 0;
        while let Some(id) = pending.pop() {
            visited += 1;
            if visited > MAX_CORNER_NODES {
                return false;
            }
            let Some(node) = self.node(id) else { continue };
            if matches!(node.kind, NodeKind::Rectangle) {
                return true;
            }
            if visited.saturating_add(pending.len()).saturating_add(node.children.len()) > MAX_CORNER_NODES {
                return false;
            }
            pending.extend(node.children.iter().copied());
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use crate::{BoolOp, Document, NodeId, NodeKind};

    #[test]
    fn boolean_radius_eligibility_follows_nested_rectangle_operands() {
        let mut doc = Document::new("Original eligibility");
        let page = doc.first_page().unwrap();
        let group = doc.create(page, NodeKind::BooleanOp { op: BoolOp::Subtract }, "Result", 0.0, 0.0, 100.0, 100.0).unwrap();
        doc.create(group, NodeKind::Ellipse { start: 0.0, sweep: std::f64::consts::TAU, inner: 0.0 }, "Ellipse", 0.0, 0.0, 60.0, 60.0).unwrap();
        assert!(!doc.has_corner_radius(group));
        let nested = doc.create(group, NodeKind::Group, "Nested", 0.0, 0.0, 50.0, 50.0).unwrap();
        let rectangle = doc.create(nested, NodeKind::Rectangle, "Rectangle", 0.0, 0.0, 50.0, 50.0).unwrap();
        assert!(doc.has_corner_radius(group) && doc.has_corner_radius(rectangle));
        assert!(!doc.has_corner_radius(nested));
        assert!(!doc.has_corner_radius(NodeId(u64::MAX)));
    }

    #[test]
    fn boolean_radius_eligibility_refuses_cycles_and_oversized_pending_lists() {
        let mut doc = Document::new("Hostile original eligibility");
        let page = doc.first_page().unwrap();
        let group = doc.create(page, NodeKind::BooleanOp { op: BoolOp::Union }, "Result", 0.0, 0.0, 100.0, 100.0).unwrap();
        doc.get_mut(group).unwrap().children.push(group);
        assert!(!doc.has_corner_radius(group), "a hostile cycle stops at the traversal budget");
        doc.get_mut(group).unwrap().children = vec![group; super::MAX_CORNER_NODES + 1];
        assert!(!doc.has_corner_radius(group), "an oversized list is refused before cloning it");
    }
}
