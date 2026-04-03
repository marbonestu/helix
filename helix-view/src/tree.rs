use crate::{graphics::Rect, View, ViewId};
use slotmap::SlotMap;
use std::collections::HashMap;

const MIN_SPLIT_WIDTH: u16 = 10;
const MIN_SPLIT_HEIGHT: u16 = 3;

// the dimensions are recomputed on window resize/tree change.
//
#[derive(Debug)]
pub struct Tree {
    root: ViewId,
    // (container, index inside the container)
    pub focus: ViewId,
    area: Rect,

    nodes: SlotMap<ViewId, Node>,

    // used for traversals
    stack: Vec<(ViewId, Rect)>,

    /// Non-None while a split is zoomed. Stores pre-zoom weights so unzoom restores exactly.
    zoom_state: Option<ZoomState>,
}

#[derive(Debug)]
struct ZoomState {
    /// Saved weights keyed by container ViewId, restored verbatim on unzoom.
    saved_weights: HashMap<ViewId, Vec<f64>>,
}

#[derive(Debug)]
pub struct Node {
    parent: ViewId,
    content: Content,
}

#[derive(Debug)]
pub enum Content {
    View(Box<View>),
    Container(Box<Container>),
}

impl Node {
    pub fn container(layout: Layout) -> Self {
        Self {
            parent: ViewId::default(),
            content: Content::Container(Box::new(Container::new(layout))),
        }
    }

    pub fn view(view: View) -> Self {
        Self {
            parent: ViewId::default(),
            content: Content::View(Box::new(view)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Horizontal,
    Vertical,
    // could explore stacked/tabbed
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug)]
pub struct Container {
    layout: Layout,
    children: Vec<ViewId>,
    /// Proportional weight for each child (same length as `children`). Defaults to 1.0 per child.
    weights: Vec<f64>,
    area: Rect,
}

impl Container {
    pub fn new(layout: Layout) -> Self {
        Self {
            layout,
            children: Vec::new(),
            weights: Vec::new(),
            area: Rect::default(),
        }
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn children(&self) -> &[ViewId] {
        &self.children
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::new(Layout::Vertical)
    }
}

impl Tree {
    pub fn new(area: Rect) -> Self {
        let root = Node::container(Layout::Vertical);

        let mut nodes = SlotMap::with_key();
        let root = nodes.insert(root);

        // root is it's own parent
        nodes[root].parent = root;

        Self {
            root,
            focus: root,
            area,
            nodes,
            stack: Vec::new(),
            zoom_state: None,
        }
    }

    pub fn insert(&mut self, view: View) -> ViewId {
        let focus = self.focus;
        let parent = self.nodes[focus].parent;
        let mut node = Node::view(view);
        node.parent = parent;
        let node = self.nodes.insert(node);
        self.get_mut(node).id = node;

        let container = match &mut self.nodes[parent] {
            Node {
                content: Content::Container(container),
                ..
            } => container,
            _ => unreachable!(),
        };

        // insert node after the current item if there is children already
        let pos = if container.children.is_empty() {
            0
        } else {
            let pos = container
                .children
                .iter()
                .position(|&child| child == focus)
                .unwrap();
            pos + 1
        };

        container.children.insert(pos, node);
        container.weights.insert(pos, 1.0);
        // focus the new node
        self.focus = node;

        // recalculate all the sizes
        self.recalculate();

        node
    }

    pub fn split(&mut self, view: View, layout: Layout) -> ViewId {
        self.unzoom();
        let focus = self.focus;
        let parent = self.nodes[focus].parent;

        let node = Node::view(view);
        let node = self.nodes.insert(node);
        self.get_mut(node).id = node;

        let container = match &mut self.nodes[parent] {
            Node {
                content: Content::Container(container),
                ..
            } => container,
            _ => unreachable!(),
        };
        if container.layout == layout {
            // insert node after the current item if there is children already
            let pos = if container.children.is_empty() {
                0
            } else {
                let pos = container
                    .children
                    .iter()
                    .position(|&child| child == focus)
                    .unwrap();
                pos + 1
            };
            container.children.insert(pos, node);
            container.weights.insert(pos, 1.0);
            self.nodes[node].parent = parent;
        } else {
            let mut split = Node::container(layout);
            split.parent = parent;
            let split = self.nodes.insert(split);

            let container = match &mut self.nodes[split] {
                Node {
                    content: Content::Container(container),
                    ..
                } => container,
                _ => unreachable!(),
            };
            container.children.push(focus);
            container.children.push(node);
            container.weights.push(1.0);
            container.weights.push(1.0);
            self.nodes[focus].parent = split;
            self.nodes[node].parent = split;

            let container = match &mut self.nodes[parent] {
                Node {
                    content: Content::Container(container),
                    ..
                } => container,
                _ => unreachable!(),
            };

            let pos = container
                .children
                .iter()
                .position(|&child| child == focus)
                .unwrap();

            // replace focus on parent with split
            container.children[pos] = split;
        }

        // focus the new node
        self.focus = node;

        // recalculate all the sizes
        self.recalculate();

        node
    }

    /// Get a mutable reference to a [Container] by index.
    /// # Panics
    /// Panics if `index` is not in self.nodes, or if the node's content is not a [Content::Container].
    fn container_mut(&mut self, index: ViewId) -> &mut Container {
        match &mut self.nodes[index] {
            Node {
                content: Content::Container(container),
                ..
            } => container,
            _ => unreachable!(),
        }
    }

    /// Resize the focused view by transferring weight to/from an adjacent sibling.
    ///
    /// `direction` controls both the layout axis and which sibling absorbs the change:
    /// `Right`/`Down` grow the focused view; `Left`/`Up` shrink it.
    /// Walks up the tree to find the nearest ancestor container with the matching axis.
    pub fn resize_view(&mut self, direction: Direction, amount: f64) {
        let focus = self.focus;
        let target_layout = match direction {
            Direction::Left | Direction::Right => Layout::Vertical,
            Direction::Up | Direction::Down => Layout::Horizontal,
        };
        let grows = matches!(direction, Direction::Right | Direction::Down);

        let mut current = focus;
        loop {
            let parent_id = self.nodes[current].parent;
            if parent_id == current {
                return; // reached root — no matching container found
            }
            let container = match &self.nodes[parent_id].content {
                Content::Container(c) => c,
                _ => unreachable!(),
            };
            if container.layout == target_layout {
                let pos = container
                    .children
                    .iter()
                    .position(|&id| id == current)
                    .unwrap();
                // Preferred sibling: right/below when growing, left/above when shrinking.
                // If the preferred side has no sibling, fall back to the opposite side so
                // the command always has an effect as long as any neighbour exists.
                let sibling_pos = if grows {
                    if pos + 1 < container.children.len() {
                        pos + 1
                    } else if pos > 0 {
                        pos - 1
                    } else {
                        return; // single child — nothing to take from
                    }
                } else if pos > 0 {
                    pos - 1
                } else if pos + 1 < container.children.len() {
                    pos + 1
                } else {
                    return; // single child — nothing to give to
                };

                let container = self.container_mut(parent_id);
                // Grow: take weight from the sibling and give to self.
                // Shrink: take weight from self and give to the sibling.
                let (donor, recipient) = if grows { (sibling_pos, pos) } else { (pos, sibling_pos) };
                let max_transfer = container.weights[donor] - 0.1;
                let transfer = amount.min(max_transfer);
                if transfer <= 0.0 {
                    return;
                }
                container.weights[donor] -= transfer;
                container.weights[recipient] += transfer;

                self.recalculate();
                return;
            }
            current = parent_id;
        }
    }

    /// Reset all splits in the tree to equal sizes by setting every container's
    /// weights to uniform 1.0 values, recursively.
    pub fn equalize_splits(&mut self) {
        self.equalize_recursive(self.root);
        self.recalculate();
    }

    fn equalize_recursive(&mut self, node_id: ViewId) {
        // Clone children before recursing: the mutable borrow of `self.nodes[node_id]`
        // must be released before we can call `self.equalize_recursive` again.
        let children = match &mut self.nodes[node_id].content {
            Content::Container(container) => {
                container.weights = vec![1.0; container.children.len()];
                container.children.clone()
            }
            Content::View(_) => return,
        };
        for child in children {
            self.equalize_recursive(child);
        }
    }

    /// Toggle zoom on the focused split. If already zoomed, unzoom; otherwise zoom.
    pub fn toggle_zoom(&mut self) {
        if self.zoom_state.is_some() {
            self.unzoom();
        } else {
            self.zoom();
        }
    }

    pub fn is_zoomed(&self) -> bool {
        self.zoom_state.is_some()
    }

    fn zoom(&mut self) {
        let focus = self.focus;
        let mut saved_weights = HashMap::new();

        let mut current = focus;
        loop {
            let parent_id = self.nodes[current].parent;
            if parent_id == current {
                break; // reached root
            }
            let container = match &self.nodes[parent_id].content {
                Content::Container(c) => c,
                _ => unreachable!(),
            };

            saved_weights.insert(parent_id, container.weights.clone());

            let pos = container
                .children
                .iter()
                .position(|&id| id == current)
                .unwrap();

            let container = self.container_mut(parent_id);
            for (i, w) in container.weights.iter_mut().enumerate() {
                *w = if i == pos { 10.0 } else { 0.1 };
            }

            current = parent_id;
        }

        self.zoom_state = Some(ZoomState { saved_weights });

        self.recalculate();
    }

    fn unzoom(&mut self) {
        let state = match self.zoom_state.take() {
            Some(s) => s,
            None => return,
        };

        for (container_id, weights) in state.saved_weights {
            if let Some(node) = self.nodes.get_mut(container_id) {
                if let Content::Container(container) = &mut node.content {
                    container.weights = weights;
                }
            }
        }

        self.recalculate();
    }

    fn remove_or_replace(&mut self, child: ViewId, replacement: Option<ViewId>) {
        let parent = self.nodes[child].parent;

        self.nodes.remove(child);

        let container = self.container_mut(parent);
        let pos = container
            .children
            .iter()
            .position(|&item| item == child)
            .unwrap();

        if let Some(new) = replacement {
            container.children[pos] = new;
            // weight is inherited at the same position; no change needed
            self.nodes[new].parent = parent;
        } else {
            container.children.remove(pos);
            container.weights.remove(pos);
        }
    }

    pub fn remove(&mut self, index: ViewId) {
        self.unzoom();
        if self.focus == index {
            // focus on something else
            self.focus = self.prev();
        }

        let parent = self.nodes[index].parent;
        let parent_is_root = parent == self.root;

        self.remove_or_replace(index, None);

        let parent_container = self.container_mut(parent);
        if parent_container.children.len() == 1 && !parent_is_root {
            // Merge the only remaining child back into its grandparent.
            let sibling = parent_container.children.pop().unwrap();
            parent_container.weights.pop();
            self.remove_or_replace(parent, Some(sibling));
        }

        self.recalculate()
    }

    pub fn views(&self) -> impl Iterator<Item = (&View, bool)> {
        let focus = self.focus;
        self.nodes.iter().filter_map(move |(key, node)| match node {
            Node {
                content: Content::View(view),
                ..
            } => Some((view.as_ref(), focus == key)),
            _ => None,
        })
    }

    pub fn views_mut(&mut self) -> impl Iterator<Item = (&mut View, bool)> {
        let focus = self.focus;
        self.nodes
            .iter_mut()
            .filter_map(move |(key, node)| match node {
                Node {
                    content: Content::View(view),
                    ..
                } => Some((view.as_mut(), focus == key)),
                _ => None,
            })
    }

    /// Get reference to a [View] by index.
    /// # Panics
    ///
    /// Panics if `index` is not in self.nodes, or if the node's content is not [Content::View]. This can be checked with [Self::contains].
    pub fn get(&self, index: ViewId) -> &View {
        self.try_get(index).unwrap()
    }

    /// Try to get reference to a [View] by index. Returns `None` if node content is not a [`Content::View`].
    ///
    /// Does not panic if the view does not exists anymore.
    pub fn try_get(&self, index: ViewId) -> Option<&View> {
        match self.nodes.get(index) {
            Some(Node {
                content: Content::View(view),
                ..
            }) => Some(view),
            _ => None,
        }
    }

    /// Get a mutable reference to a [View] by index.
    /// # Panics
    ///
    /// Panics if `index` is not in self.nodes, or if the node's content is not [Content::View]. This can be checked with [Self::contains].
    pub fn get_mut(&mut self, index: ViewId) -> &mut View {
        match &mut self.nodes[index] {
            Node {
                content: Content::View(view),
                ..
            } => view,
            _ => unreachable!(),
        }
    }

    /// Check if tree contains a [Node] with a given index.
    pub fn contains(&self, index: ViewId) -> bool {
        self.nodes.contains_key(index)
    }

    pub fn is_empty(&self) -> bool {
        match &self.nodes[self.root] {
            Node {
                content: Content::Container(container),
                ..
            } => container.children.is_empty(),
            _ => unreachable!(),
        }
    }

    pub fn resize(&mut self, area: Rect) -> bool {
        if self.area != area {
            self.area = area;
            self.recalculate();
            return true;
        }
        false
    }

    pub fn recalculate(&mut self) {
        if self.is_empty() {
            // There are no more views, so the tree should focus itself again.
            self.focus = self.root;

            return;
        }

        self.stack.push((self.root, self.area));

        // take the area
        // fetch the node
        // a) node is view, give it whole area
        // b) node is container, calculate areas for each child and push them on the stack

        while let Some((key, area)) = self.stack.pop() {
            let node = &mut self.nodes[key];

            match &mut node.content {
                Content::View(view) => {
                    // debug!!("setting view area {:?}", area);
                    view.area = area;
                } // TODO: call f()
                Content::Container(container) => {
                    // debug!!("setting container area {:?}", area);
                    container.area = area;

                    match container.layout {
                        Layout::Horizontal => {
                            let len = container.children.len();
                            let total_weight: f64 = container.weights.iter().sum();
                            let available_height = area.height as f64;
                            let mut child_y = area.y;

                            for (i, (child, &weight)) in container
                                .children
                                .iter()
                                .zip(container.weights.iter())
                                .enumerate()
                            {
                                let height = if i == len - 1 {
                                    // last child absorbs rounding remainder; no min clamp here
                                    (container.area.y + container.area.height)
                                        .saturating_sub(child_y)
                                } else {
                                    let h = ((weight / total_weight) * available_height).round()
                                        as u16;
                                    h.max(MIN_SPLIT_HEIGHT)
                                };
                                let child_area = Rect::new(
                                    container.area.x,
                                    child_y,
                                    container.area.width,
                                    height,
                                );
                                child_y += height;
                                self.stack.push((*child, child_area));
                            }
                        }
                        Layout::Vertical => {
                            let len = container.children.len();
                            let total_weight: f64 = container.weights.iter().sum();
                            let inner_gap = 1u16;
                            let total_gap = inner_gap * (len as u16).saturating_sub(1);
                            let used_area = area.width.saturating_sub(total_gap) as f64;
                            let mut child_x = area.x;

                            for (i, (child, &weight)) in container
                                .children
                                .iter()
                                .zip(container.weights.iter())
                                .enumerate()
                            {
                                let width = if i == len - 1 {
                                    // last child absorbs rounding remainder; no min clamp here
                                    (container.area.x + container.area.width)
                                        .saturating_sub(child_x)
                                } else {
                                    let w = ((weight / total_weight) * used_area).round() as u16;
                                    w.max(MIN_SPLIT_WIDTH)
                                };
                                let child_area = Rect::new(
                                    child_x,
                                    container.area.y,
                                    width,
                                    container.area.height,
                                );
                                child_x += width + inner_gap;
                                self.stack.push((*child, child_area));
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn traverse(&self) -> Traverse<'_> {
        Traverse::new(self)
    }

    // Finds the split in the given direction if it exists
    pub fn find_split_in_direction(&self, id: ViewId, direction: Direction) -> Option<ViewId> {
        let parent = self.nodes[id].parent;
        // Base case, we found the root of the tree
        if parent == id {
            return None;
        }
        // Parent must always be a container
        let parent_container = match &self.nodes[parent].content {
            Content::Container(container) => container,
            Content::View(_) => unreachable!(),
        };

        match (direction, parent_container.layout) {
            (Direction::Up, Layout::Vertical)
            | (Direction::Left, Layout::Horizontal)
            | (Direction::Right, Layout::Horizontal)
            | (Direction::Down, Layout::Vertical) => {
                // The desired direction of movement is not possible within
                // the parent container so the search must continue closer to
                // the root of the split tree.
                self.find_split_in_direction(parent, direction)
            }
            (Direction::Up, Layout::Horizontal)
            | (Direction::Down, Layout::Horizontal)
            | (Direction::Left, Layout::Vertical)
            | (Direction::Right, Layout::Vertical) => {
                // It's possible to move in the desired direction within
                // the parent container so an attempt is made to find the
                // correct child.
                match self.find_child(id, &parent_container.children, direction) {
                    // Child is found, search is ended
                    Some(id) => Some(id),
                    // A child is not found. This could be because of either two scenarios
                    // 1. Its not possible to move in the desired direction, and search should end
                    // 2. A layout like the following with focus at X and desired direction Right
                    // | _ | x |   |
                    // | _ _ _ |   |
                    // | _ _ _ |   |
                    // The container containing X ends at X so no rightward movement is possible
                    // however there still exists another view/container to the right that hasn't
                    // been explored. Thus another search is done here in the parent container
                    // before concluding it's not possible to move in the desired direction.
                    None => self.find_split_in_direction(parent, direction),
                }
            }
        }
    }

    fn find_child(&self, id: ViewId, children: &[ViewId], direction: Direction) -> Option<ViewId> {
        let mut child_id = match direction {
            // index wise in the child list the Up and Left represents a -1
            // thus reversed iterator.
            Direction::Up | Direction::Left => children
                .iter()
                .rev()
                .skip_while(|i| **i != id)
                .copied()
                .nth(1)?,
            // Down and Right => +1 index wise in the child list
            Direction::Down | Direction::Right => {
                children.iter().skip_while(|i| **i != id).copied().nth(1)?
            }
        };
        let (current_x, current_y) = match &self.nodes[self.focus].content {
            Content::View(current_view) => (current_view.area.left(), current_view.area.top()),
            Content::Container(_) => unreachable!(),
        };

        // If the child is a container the search finds the closest container child
        // visually based on screen location.
        while let Content::Container(container) = &self.nodes[child_id].content {
            match (direction, container.layout) {
                (_, Layout::Vertical) => {
                    // find closest split based on x because y is irrelevant
                    // in a vertical container (and already correct based on previous search)
                    child_id = *container.children.iter().min_by_key(|id| {
                        let x = match &self.nodes[**id].content {
                            Content::View(view) => view.area.left(),
                            Content::Container(container) => container.area.left(),
                        };
                        (current_x as i16 - x as i16).abs()
                    })?;
                }
                (_, Layout::Horizontal) => {
                    // find closest split based on y because x is irrelevant
                    // in a horizontal container (and already correct based on previous search)
                    child_id = *container.children.iter().min_by_key(|id| {
                        let y = match &self.nodes[**id].content {
                            Content::View(view) => view.area.top(),
                            Content::Container(container) => container.area.top(),
                        };
                        (current_y as i16 - y as i16).abs()
                    })?;
                }
            }
        }
        Some(child_id)
    }

    pub fn prev(&self) -> ViewId {
        // This function is very dumb, but that's because we don't store any parent links.
        // (we'd be able to go parent.prev_sibling() recursively until we find something)
        // For now that's okay though, since it's unlikely you'll be able to open a large enough
        // number of splits to notice.

        let mut views = self
            .traverse()
            .rev()
            .skip_while(|&(id, _view)| id != self.focus)
            .skip(1); // Skip focused value
        if let Some((id, _)) = views.next() {
            id
        } else {
            // extremely crude, take the last item
            let (key, _) = self.traverse().next_back().unwrap();
            key
        }
    }

    pub fn next(&self) -> ViewId {
        // This function is very dumb, but that's because we don't store any parent links.
        // (we'd be able to go parent.next_sibling() recursively until we find something)
        // For now that's okay though, since it's unlikely you'll be able to open a large enough
        // number of splits to notice.

        let mut views = self
            .traverse()
            .skip_while(|&(id, _view)| id != self.focus)
            .skip(1); // Skip focused value
        if let Some((id, _)) = views.next() {
            id
        } else {
            // extremely crude, take the first item again
            let (key, _) = self.traverse().next().unwrap();
            key
        }
    }

    pub fn transpose(&mut self) {
        let focus = self.focus;
        let parent = self.nodes[focus].parent;
        if let Content::Container(container) = &mut self.nodes[parent].content {
            container.layout = match container.layout {
                Layout::Vertical => Layout::Horizontal,
                Layout::Horizontal => Layout::Vertical,
            };
            self.recalculate();
        }
    }

    pub fn swap_split_in_direction(&mut self, direction: Direction) -> Option<()> {
        let focus = self.focus;
        let target = self.find_split_in_direction(focus, direction)?;
        let focus_parent = self.nodes[focus].parent;
        let target_parent = self.nodes[target].parent;

        if focus_parent == target_parent {
            let parent = focus_parent;
            let [parent, focus, target] = self.nodes.get_disjoint_mut([parent, focus, target])?;
            match (&mut parent.content, &mut focus.content, &mut target.content) {
                (
                    Content::Container(parent),
                    Content::View(focus_view),
                    Content::View(target_view),
                ) => {
                    let focus_pos = parent.children.iter().position(|id| focus_view.id == *id)?;
                    let target_pos = parent
                        .children
                        .iter()
                        .position(|id| target_view.id == *id)?;
                    // swap node positions so that traversal order is kept
                    parent.children[focus_pos] = target_view.id;
                    parent.children[target_pos] = focus_view.id;
                    // weights follow their views
                    parent.weights.swap(focus_pos, target_pos);
                    // swap area so that views rendered at the correct location
                    std::mem::swap(&mut focus_view.area, &mut target_view.area);

                    Some(())
                }
                _ => unreachable!(),
            }
        } else {
            let [focus_parent, target_parent, focus, target] =
                self.nodes
                    .get_disjoint_mut([focus_parent, target_parent, focus, target])?;
            match (
                &mut focus_parent.content,
                &mut target_parent.content,
                &mut focus.content,
                &mut target.content,
            ) {
                (
                    Content::Container(focus_parent),
                    Content::Container(target_parent),
                    Content::View(focus_view),
                    Content::View(target_view),
                ) => {
                    let focus_pos = focus_parent
                        .children
                        .iter()
                        .position(|id| focus_view.id == *id)?;
                    let target_pos = target_parent
                        .children
                        .iter()
                        .position(|id| target_view.id == *id)?;
                    // re-parent target and focus nodes
                    std::mem::swap(
                        &mut focus_parent.children[focus_pos],
                        &mut target_parent.children[target_pos],
                    );
                    // weights follow their views across parents
                    std::mem::swap(
                        &mut focus_parent.weights[focus_pos],
                        &mut target_parent.weights[target_pos],
                    );
                    std::mem::swap(&mut focus.parent, &mut target.parent);
                    // swap area so that views rendered at the correct location
                    std::mem::swap(&mut focus_view.area, &mut target_view.area);

                    Some(())
                }
                _ => unreachable!(),
            }
        }
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    pub fn root(&self) -> ViewId {
        self.root
    }

    /// Access the content of a node by its id.
    pub fn node_content(&self, id: ViewId) -> &Content {
        &self.nodes[id].content
    }
}

#[derive(Debug)]
pub struct Traverse<'a> {
    tree: &'a Tree,
    stack: Vec<ViewId>, // TODO: reuse the one we use on update
}

impl<'a> Traverse<'a> {
    fn new(tree: &'a Tree) -> Self {
        Self {
            tree,
            stack: vec![tree.root],
        }
    }
}

impl<'a> Iterator for Traverse<'a> {
    type Item = (ViewId, &'a View);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let key = self.stack.pop()?;

            let node = &self.tree.nodes[key];

            match &node.content {
                Content::View(view) => return Some((key, view)),
                Content::Container(container) => {
                    self.stack.extend(container.children.iter().rev());
                }
            }
        }
    }
}

impl DoubleEndedIterator for Traverse<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            let key = self.stack.pop()?;

            let node = &self.tree.nodes[key];

            match &node.content {
                Content::View(view) => return Some((key, view)),
                Content::Container(container) => {
                    self.stack.extend(container.children.iter());
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::editor::GutterConfig;
    use crate::DocumentId;

    #[test]
    fn find_split_in_direction() {
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: 180,
            height: 80,
        });
        let mut view = View::new(DocumentId::default(), GutterConfig::default());
        view.area = Rect::new(0, 0, 180, 80);
        tree.insert(view);

        let l0 = tree.focus;
        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Vertical);
        let r0 = tree.focus;

        tree.focus = l0;
        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Horizontal);
        let l1 = tree.focus;

        tree.focus = l0;
        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Vertical);

        // Tree in test
        // | L0  | L2 |    |
        // |    L1    | R0 |
        let l2 = tree.focus;
        assert_eq!(Some(l0), tree.find_split_in_direction(l2, Direction::Left));
        assert_eq!(Some(l1), tree.find_split_in_direction(l2, Direction::Down));
        assert_eq!(Some(r0), tree.find_split_in_direction(l2, Direction::Right));
        assert_eq!(None, tree.find_split_in_direction(l2, Direction::Up));

        tree.focus = l1;
        assert_eq!(None, tree.find_split_in_direction(l1, Direction::Left));
        assert_eq!(None, tree.find_split_in_direction(l1, Direction::Down));
        assert_eq!(Some(r0), tree.find_split_in_direction(l1, Direction::Right));
        assert_eq!(Some(l0), tree.find_split_in_direction(l1, Direction::Up));

        tree.focus = l0;
        assert_eq!(None, tree.find_split_in_direction(l0, Direction::Left));
        assert_eq!(Some(l1), tree.find_split_in_direction(l0, Direction::Down));
        assert_eq!(Some(l2), tree.find_split_in_direction(l0, Direction::Right));
        assert_eq!(None, tree.find_split_in_direction(l0, Direction::Up));

        tree.focus = r0;
        assert_eq!(Some(l2), tree.find_split_in_direction(r0, Direction::Left));
        assert_eq!(None, tree.find_split_in_direction(r0, Direction::Down));
        assert_eq!(None, tree.find_split_in_direction(r0, Direction::Right));
        assert_eq!(None, tree.find_split_in_direction(r0, Direction::Up));
    }

    #[test]
    fn swap_split_in_direction() {
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: 180,
            height: 80,
        });

        let doc_l0 = DocumentId::default();
        let mut view = View::new(doc_l0, GutterConfig::default());
        view.area = Rect::new(0, 0, 180, 80);
        tree.insert(view);

        let l0 = tree.focus;

        let doc_r0 = DocumentId::default();
        let view = View::new(doc_r0, GutterConfig::default());
        tree.split(view, Layout::Vertical);
        let r0 = tree.focus;

        tree.focus = l0;

        let doc_l1 = DocumentId::default();
        let view = View::new(doc_l1, GutterConfig::default());
        tree.split(view, Layout::Horizontal);
        let l1 = tree.focus;

        tree.focus = l0;

        let doc_l2 = DocumentId::default();
        let view = View::new(doc_l2, GutterConfig::default());
        tree.split(view, Layout::Vertical);
        let l2 = tree.focus;

        // Views in test
        // | L0  | L2 |    |
        // |    L1    | R0 |

        // Document IDs in test
        // | l0  | l2 |    |
        // |    l1    | r0 |

        fn doc_id(tree: &Tree, view_id: ViewId) -> Option<DocumentId> {
            if let Content::View(view) = &tree.nodes[view_id].content {
                Some(view.doc)
            } else {
                None
            }
        }

        tree.focus = l0;
        // `*` marks the view in focus from view table (here L0)
        // | l0*  | l2 |    |
        // |    l1     | r0 |
        tree.swap_split_in_direction(Direction::Down);
        // | l1   | l2 |    |
        // |    l0*    | r0 |
        assert_eq!(tree.focus, l0);
        assert_eq!(doc_id(&tree, l0), Some(doc_l1));
        assert_eq!(doc_id(&tree, l1), Some(doc_l0));
        assert_eq!(doc_id(&tree, l2), Some(doc_l2));
        assert_eq!(doc_id(&tree, r0), Some(doc_r0));

        tree.swap_split_in_direction(Direction::Right);

        // | l1  | l2 |     |
        // |    r0    | l0* |
        assert_eq!(tree.focus, l0);
        assert_eq!(doc_id(&tree, l0), Some(doc_l1));
        assert_eq!(doc_id(&tree, l1), Some(doc_r0));
        assert_eq!(doc_id(&tree, l2), Some(doc_l2));
        assert_eq!(doc_id(&tree, r0), Some(doc_l0));

        // cannot swap, nothing changes
        tree.swap_split_in_direction(Direction::Up);
        // | l1  | l2 |     |
        // |    r0    | l0* |
        assert_eq!(tree.focus, l0);
        assert_eq!(doc_id(&tree, l0), Some(doc_l1));
        assert_eq!(doc_id(&tree, l1), Some(doc_r0));
        assert_eq!(doc_id(&tree, l2), Some(doc_l2));
        assert_eq!(doc_id(&tree, r0), Some(doc_l0));

        // cannot swap, nothing changes
        tree.swap_split_in_direction(Direction::Down);
        // | l1  | l2 |     |
        // |    r0    | l0* |
        assert_eq!(tree.focus, l0);
        assert_eq!(doc_id(&tree, l0), Some(doc_l1));
        assert_eq!(doc_id(&tree, l1), Some(doc_r0));
        assert_eq!(doc_id(&tree, l2), Some(doc_l2));
        assert_eq!(doc_id(&tree, r0), Some(doc_l0));

        tree.focus = l2;
        // | l1  | l2* |    |
        // |    r0     | l0 |

        tree.swap_split_in_direction(Direction::Down);
        // | l1  | r0  |    |
        // |    l2*    | l0 |
        assert_eq!(tree.focus, l2);
        assert_eq!(doc_id(&tree, l0), Some(doc_l1));
        assert_eq!(doc_id(&tree, l1), Some(doc_l2));
        assert_eq!(doc_id(&tree, l2), Some(doc_r0));
        assert_eq!(doc_id(&tree, r0), Some(doc_l0));

        tree.swap_split_in_direction(Direction::Up);
        // | l2* | r0 |    |
        // |    l1    | l0 |
        assert_eq!(tree.focus, l2);
        assert_eq!(doc_id(&tree, l0), Some(doc_l2));
        assert_eq!(doc_id(&tree, l1), Some(doc_l1));
        assert_eq!(doc_id(&tree, l2), Some(doc_r0));
        assert_eq!(doc_id(&tree, r0), Some(doc_l0));
    }

    #[test]
    fn all_vertical_views_have_same_width() {
        let tree_area_width = 180;
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: tree_area_width,
            height: 80,
        });
        let mut view = View::new(DocumentId::default(), GutterConfig::default());
        view.area = Rect::new(0, 0, 180, 80);
        tree.insert(view);

        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Vertical);

        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Horizontal);

        tree.remove(tree.focus);

        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Vertical);

        // Make sure that we only have one level in the tree.
        assert_eq!(3, tree.views().count());
        assert_eq!(
            vec![
                tree_area_width / 3 - 1, // gap here
                tree_area_width / 3 - 1, // gap here
                tree_area_width / 3
            ],
            tree.views()
                .map(|(view, _)| view.area.width)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn vsplit_gap_rounding() {
        // 200 cols / 10 views: each gets 19px (well above MIN_SPLIT_WIDTH), last absorbs
        // the 1px rounding remainder and gets 20px.
        let (tree_area_width, tree_area_height) = (200, 24);
        let mut tree = Tree::new(Rect {
            x: 0,
            y: 0,
            width: tree_area_width,
            height: tree_area_height,
        });
        let mut view = View::new(DocumentId::default(), GutterConfig::default());
        view.area = Rect::new(0, 0, tree_area_width, tree_area_height);
        tree.insert(view);

        for _ in 0..9 {
            let view = View::new(DocumentId::default(), GutterConfig::default());
            tree.split(view, Layout::Vertical);
        }

        assert_eq!(10, tree.views().count());
        assert_eq!(
            std::iter::repeat_n(19, 9)
                .chain(Some(20)) // last child absorbs rounding remainder
                .collect::<Vec<_>>(),
            tree.views()
                .map(|(view, _)| view.area.width)
                .collect::<Vec<_>>()
        );
    }

    fn make_tree(width: u16, height: u16) -> Tree {
        let mut tree = Tree::new(Rect::new(0, 0, width, height));
        let mut view = View::new(DocumentId::default(), GutterConfig::default());
        view.area = Rect::new(0, 0, width, height);
        tree.insert(view);
        tree
    }

    fn add_vsplit(tree: &mut Tree) -> ViewId {
        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Vertical);
        tree.focus
    }

    fn add_hsplit(tree: &mut Tree) -> ViewId {
        let view = View::new(DocumentId::default(), GutterConfig::default());
        tree.split(view, Layout::Horizontal);
        tree.focus
    }

    fn container_weights(tree: &Tree, node_id: ViewId) -> Vec<f64> {
        match &tree.nodes[node_id].content {
            Content::Container(c) => c.weights.clone(),
            _ => panic!("not a container"),
        }
    }

    fn root_container_weights(tree: &Tree) -> Vec<f64> {
        container_weights(tree, tree.root)
    }

    // ── Weight invariants ─────────────────────────────────────────────────────

    #[test]
    fn weights_initialized_to_equal() {
        let mut tree = make_tree(180, 80);
        add_vsplit(&mut tree);
        add_vsplit(&mut tree);
        let weights = root_container_weights(&tree);
        assert_eq!(weights.len(), 3);
        assert!(weights.iter().all(|&w| (w - 1.0).abs() < f64::EPSILON));
    }

    #[test]
    fn weights_sync_on_remove() {
        let mut tree = make_tree(180, 80);
        let _a = tree.focus;
        let b = add_vsplit(&mut tree);
        let _c = add_vsplit(&mut tree);
        assert_eq!(root_container_weights(&tree).len(), 3);

        tree.remove(b);
        let weights = root_container_weights(&tree);
        assert_eq!(weights.len(), 2);
    }

    #[test]
    fn weights_swap_same_parent() {
        let mut tree = make_tree(180, 80);
        let a = tree.focus;
        add_vsplit(&mut tree);
        let _b = tree.focus;

        // Give a bigger weight to 'a'
        let root = tree.root;
        if let Content::Container(c) = &mut tree.nodes[root].content {
            c.weights[0] = 2.0;
            c.weights[1] = 1.0;
        }

        tree.focus = a;
        tree.swap_split_in_direction(Direction::Right);

        // After swap, 'a' is at position 1, 'b' is at position 0.
        // Weights should have swapped too.
        let weights = root_container_weights(&tree);
        assert!((weights[0] - 1.0).abs() < f64::EPSILON);
        assert!((weights[1] - 2.0).abs() < f64::EPSILON);
    }

    // ── Proportional layout ───────────────────────────────────────────────────

    #[test]
    fn equal_weights_match_equal_distribution() {
        // Backwards-compatibility: equal weights must produce equal-ish widths.
        let mut tree = make_tree(180, 80);
        add_vsplit(&mut tree);
        add_vsplit(&mut tree);
        let widths: Vec<_> = tree.views().map(|(v, _)| v.area.width).collect();
        assert_eq!(widths.len(), 3);
        let max = *widths.iter().max().unwrap();
        let min = *widths.iter().min().unwrap();
        assert!(max - min <= 1, "widths should be equal (±1 rounding): {widths:?}");
    }

    #[test]
    fn weighted_vertical_distribution() {
        // 2:1 weights → first view gets ~2/3 of usable width.
        let mut tree = make_tree(181, 80);
        let a = tree.focus;
        add_vsplit(&mut tree);

        let root = tree.root;
        if let Content::Container(c) = &mut tree.nodes[root].content {
            c.weights[0] = 2.0;
            c.weights[1] = 1.0;
        }
        tree.recalculate();

        tree.focus = a;
        let views: Vec<_> = tree.views().collect();
        let (focused, _) = views.iter().find(|(_, f)| *f).unwrap();
        let (other, _) = views.iter().find(|(_, f)| !f).unwrap();
        assert!(
            focused.area.width > other.area.width,
            "focused ({}) should be wider than other ({})",
            focused.area.width,
            other.area.width
        );
    }

    #[test]
    fn weighted_horizontal_distribution() {
        // 1:2 weights → second view gets ~2/3 of height.
        // add_hsplit from root (Vertical) creates a Horizontal sub-container.
        let mut tree = make_tree(180, 90);
        let a = tree.focus;
        add_hsplit(&mut tree);

        // The horizontal container is a's parent (not root).
        let hcontainer = tree.nodes[a].parent;
        if let Content::Container(c) = &mut tree.nodes[hcontainer].content {
            c.weights[0] = 1.0;
            c.weights[1] = 2.0;
        }
        tree.recalculate();

        tree.focus = a;
        let views: Vec<_> = tree.views().collect();
        let (focused, _) = views.iter().find(|(_, f)| *f).unwrap();
        let (other, _) = views.iter().find(|(_, f)| !f).unwrap();
        assert!(
            other.area.height > focused.area.height,
            "second view ({}) should be taller than first ({})",
            other.area.height,
            focused.area.height
        );
    }

    #[test]
    fn last_child_absorbs_rounding() {
        let mut tree = make_tree(100, 80);
        add_vsplit(&mut tree);
        add_vsplit(&mut tree);

        let total_width: u16 = tree.views().map(|(v, _)| v.area.width).sum();
        let gap_total = 2u16; // 3 children → 2 gaps of 1px each
        assert_eq!(
            total_width + gap_total,
            100,
            "widths + gaps should exactly fill container"
        );
    }

    #[test]
    fn vertical_gap_count_fix() {
        // n children have n-1 separators. Total of widths + gaps must equal container width.
        let mut tree = make_tree(100, 80);
        add_vsplit(&mut tree);
        add_vsplit(&mut tree);

        let n = tree.views().count() as u16;
        let total_width: u16 = tree.views().map(|(v, _)| v.area.width).sum();
        assert_eq!(
            total_width + (n - 1),
            100,
            "expected {total_width} + {} gaps = 100",
            n - 1
        );
    }

    // ── Resize API ────────────────────────────────────────────────────────────

    #[test]
    fn resize_view_grows_right() {
        let mut tree = make_tree(180, 80);
        let a = tree.focus;
        add_vsplit(&mut tree);

        tree.focus = a;
        let width_before = tree.get(a).area.width;
        tree.resize_view(Direction::Right, 0.4);
        let width_after = tree.get(a).area.width;

        assert!(
            width_after > width_before,
            "view should be wider after grow_right: {width_before} → {width_after}"
        );
    }

    #[test]
    fn resize_view_fallback_to_opposite_sibling() {
        // When the preferred sibling doesn't exist, the resize falls back to
        // the opposite side so grow/shrink always has an effect if any neighbour exists.
        let mut tree = make_tree(180, 80);
        let a = tree.focus;
        let b = add_vsplit(&mut tree);

        // b is rightmost — growing right falls back to taking from a (left sibling)
        tree.focus = b;
        tree.resize_view(Direction::Right, 0.4);
        let weights = root_container_weights(&tree);
        assert!(weights[1] > 1.0, "b should have grown via fallback");
        assert!(weights[0] < 1.0, "a should have shrunk as donor");

        // a is leftmost — shrinking left falls back to giving to b (right sibling)
        tree.focus = a;
        let w_a_before = root_container_weights(&tree)[0];
        tree.resize_view(Direction::Left, 0.2);
        let weights = root_container_weights(&tree);
        assert!(weights[0] < w_a_before, "a should have shrunk via fallback");
    }

    #[test]
    fn resize_view_no_op_single_view() {
        let mut tree = make_tree(180, 80);
        let width_before = tree.get(tree.focus).area.width;
        tree.resize_view(Direction::Right, 0.4);
        let width_after = tree.get(tree.focus).area.width;
        assert_eq!(width_before, width_after);
    }

    #[test]
    fn resize_view_respects_min_weight() {
        let mut tree = make_tree(180, 80);
        let a = tree.focus;
        add_vsplit(&mut tree);

        // Set sibling weight just above the minimum
        let root = tree.root;
        if let Content::Container(c) = &mut tree.nodes[root].content {
            c.weights[0] = 1.0;
            c.weights[1] = 0.2;
        }

        tree.focus = a;
        tree.resize_view(Direction::Right, 5.0); // tries to take more than available

        let weights = root_container_weights(&tree);
        assert!(
            weights[1] >= 0.1,
            "sibling weight should not go below 0.1, got {}",
            weights[1]
        );
    }

    #[test]
    fn resize_view_walks_up_tree() {
        // Layout: root→[hcontainer→[vcontainer→[a, b], bottom]]
        // Focusing 'a' and growing height should walk past vcontainer to hcontainer.
        let mut tree = make_tree(180, 80);
        let a = tree.focus;
        let _bottom = add_hsplit(&mut tree); // a's parent becomes hcontainer

        tree.focus = a;
        let _b = add_vsplit(&mut tree); // a's parent becomes vcontainer; vcontainer in hcontainer

        // hcontainer is vcontainer's parent
        let vcontainer = tree.nodes[a].parent;
        let hcontainer = tree.nodes[vcontainer].parent;
        let hweights_before = container_weights(&tree, hcontainer);

        tree.focus = a;
        tree.resize_view(Direction::Down, 0.4);

        let hweights_after = container_weights(&tree, hcontainer);
        assert_ne!(
            hweights_before, hweights_after,
            "horizontal container weights should have changed"
        );
    }

    // ── Equalize ──────────────────────────────────────────────────────────────

    #[test]
    fn equalize_splits_resets_weights() {
        let mut tree = make_tree(180, 80);
        add_vsplit(&mut tree);
        add_vsplit(&mut tree);

        let root = tree.root;
        if let Content::Container(c) = &mut tree.nodes[root].content {
            c.weights = vec![3.0, 0.5, 1.5];
        }

        tree.equalize_splits();
        let weights = root_container_weights(&tree);
        assert!(
            weights.iter().all(|&w| (w - 1.0).abs() < f64::EPSILON),
            "all weights should be 1.0 after equalize: {weights:?}"
        );
    }

    #[test]
    fn equalize_splits_recursive() {
        let mut tree = make_tree(180, 80);
        let a = tree.focus;
        add_vsplit(&mut tree); // root now has [a, b] vertical

        tree.focus = a;
        add_hsplit(&mut tree); // a splits horizontally: root→[sub, b], sub→[a, new]

        // bias the weights
        let root = tree.root;
        if let Content::Container(c) = &mut tree.nodes[root].content {
            c.weights = vec![3.0, 1.0];
        }

        tree.equalize_splits();

        // All containers at all levels should have uniform weights
        for (_, node) in tree.nodes.iter() {
            if let Content::Container(c) = &node.content {
                for &w in &c.weights {
                    assert!((w - 1.0).abs() < f64::EPSILON, "weight should be 1.0: {w}");
                }
            }
        }
    }

    // ── Zoom ──────────────────────────────────────────────────────────────────

    #[test]
    fn zoom_inflates_focused_view() {
        let mut tree = make_tree(180, 80);
        let a = tree.focus;
        add_vsplit(&mut tree);
        add_vsplit(&mut tree);

        tree.focus = a;
        tree.toggle_zoom();

        assert!(tree.is_zoomed());
        let views: Vec<_> = tree.views().collect();
        let focused_area = views.iter().find(|(_, f)| *f).unwrap().0.area;
        let others_max = views
            .iter()
            .filter(|(_, f)| !f)
            .map(|(v, _)| v.area.width)
            .max()
            .unwrap();
        assert!(
            focused_area.width > others_max * 3,
            "zoomed view ({}) should dominate siblings (max {})",
            focused_area.width,
            others_max
        );
    }

    #[test]
    fn unzoom_restores_exact_weights() {
        let mut tree = make_tree(180, 80);
        let a = tree.focus;
        add_vsplit(&mut tree);
        add_vsplit(&mut tree);

        let root = tree.root;
        if let Content::Container(c) = &mut tree.nodes[root].content {
            c.weights = vec![2.0, 1.0, 3.0];
        }
        tree.recalculate();

        tree.focus = a;
        tree.toggle_zoom();
        tree.toggle_zoom(); // unzoom

        assert!(!tree.is_zoomed());
        let weights = root_container_weights(&tree);
        assert!((weights[0] - 2.0).abs() < f64::EPSILON);
        assert!((weights[1] - 1.0).abs() < f64::EPSILON);
        assert!((weights[2] - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn zoom_then_remove_unzooms_first() {
        let mut tree = make_tree(180, 80);
        let a = tree.focus;
        let b = add_vsplit(&mut tree);

        tree.focus = a;
        tree.toggle_zoom();
        assert!(tree.is_zoomed());

        tree.remove(b);
        assert!(!tree.is_zoomed(), "zoom should be cleared after removing a view");
        assert_eq!(1, tree.views().count());
    }

    #[test]
    fn zoom_then_split_unzooms_first() {
        let mut tree = make_tree(180, 80);
        let a = tree.focus;
        add_vsplit(&mut tree);

        tree.focus = a;
        tree.toggle_zoom();
        assert!(tree.is_zoomed());

        add_vsplit(&mut tree);
        assert!(!tree.is_zoomed(), "zoom should be cleared after splitting");
        assert_eq!(3, tree.views().count());
    }

    #[test]
    fn zoom_preserves_tree_traversal() {
        let mut tree = make_tree(180, 80);
        let a = tree.focus;
        add_vsplit(&mut tree);
        add_vsplit(&mut tree);

        tree.focus = a;
        tree.toggle_zoom();

        assert_eq!(3, tree.views().count(), "all 3 views should still be present while zoomed");
    }

    // ── Terminal resize ───────────────────────────────────────────────────────

    #[test]
    fn terminal_resize_preserves_proportions() {
        let mut tree = make_tree(180, 80);
        let a = tree.focus;
        add_vsplit(&mut tree);

        let root = tree.root;
        if let Content::Container(c) = &mut tree.nodes[root].content {
            c.weights = vec![2.0, 1.0];
        }
        tree.recalculate();

        let width_a_before = tree.get(a).area.width;
        let total_before = 180u16;

        tree.resize(Rect::new(0, 0, 120, 60));

        let width_a_after = tree.get(a).area.width;
        let total_after = 120u16;

        // ratio should be preserved: width_a / total ≈ 2/3
        let ratio_before = width_a_before as f64 / total_before as f64;
        let ratio_after = width_a_after as f64 / total_after as f64;
        assert!(
            (ratio_before - ratio_after).abs() < 0.05,
            "proportion should be preserved: {ratio_before:.3} vs {ratio_after:.3}"
        );
    }
}
