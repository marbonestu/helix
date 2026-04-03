# Resizable Splits Implementation Plan

Allow users to resize editor splits via keybindings and optionally mouse drag,
replacing the current equal-distribution layout with proportional weights.
Includes zoom/unzoom to temporarily maximize a split.

## Key Insight

The `Container` in `helix-view/src/tree.rs` stores a `Vec<ViewId>` of children
and distributes space equally in `recalculate()`. Adding a parallel
`weights: Vec<f64>` and switching to proportional distribution is the minimal
change needed. Terminal resize automatically preserves proportions since weights
are ratios.

---

## Phase 1: Weight Storage

### Step 1.1: Add `weights` to `Container`

**File:** `helix-view/src/tree.rs`

```rust
#[derive(Debug)]
pub struct Container {
    layout: Layout,
    children: Vec<ViewId>,
    /// Proportional weight for each child (same length as children).
    /// Default: 1.0 per child (equal distribution).
    weights: Vec<f64>,
    area: Rect,
}
```

### Step 1.2: Keep `weights` in sync with `children`

Every mutation of `children` must also update `weights`:

| Location | Mutation | Weight change |
|----------|----------|---------------|
| `Tree::insert()` (~line 134) | `children.insert(pos, node)` | `weights.insert(pos, 1.0)` |
| `Tree::split()` (~line 171, same layout) | Insert child | `weights.insert(pos, 1.0)` |
| `Tree::split()` (~line 185, new container) | Create sub-container | Push `1.0` for each child |
| `Tree::remove_or_replace()` (~line 242) | `children.remove(pos)` | `weights.remove(pos)` |
| `Tree::remove_or_replace()` (~line 243) | `children[pos] = new` (replacement) | No change — replacement inherits weight at that position |
| `Tree::remove()` (~line 265) | `children.pop()` | `weights.pop()` |
| `Tree::swap_split_in_direction()` (~line 619, same parent) | `children[focus_pos] = target; children[target_pos] = focus` | `weights.swap(focus_pos, target_pos)` |
| `Tree::swap_split_in_direction()` (~line 654, cross-parent) | `std::mem::swap(&mut focus_parent.children[fp], &mut target_parent.children[tp])` | `std::mem::swap(&mut focus_parent.weights[fp], &mut target_parent.weights[tp])` — requires same `get_disjoint_mut` pattern already used |

---

## Phase 2: Proportional Layout

### Step 2.1: Modify `recalculate()`

**File:** `helix-view/src/tree.rs`, `recalculate()` method (~line 355)

Replace equal-distribution logic with weight-proportional distribution.

**Horizontal layout** (splits stacked vertically):

```rust
Layout::Horizontal => {
    let len = container.children.len();
    let total_weight: f64 = container.weights.iter().sum();
    let available_height = area.height as f64;
    let mut child_y = area.y;

    for (i, (child, &weight)) in container.children.iter()
        .zip(container.weights.iter()).enumerate()
    {
        let height = if i == len - 1 {
            // Last child takes remaining space (avoids rounding gaps)
            container.area.y + container.area.height - child_y
        } else {
            ((weight / total_weight) * available_height).round() as u16
        };
        let child_area = Rect::new(
            container.area.x, child_y,
            container.area.width, height,
        );
        child_y += height;
        self.stack.push((*child, child_area));
    }
}
```

**Vertical layout** (splits side by side):

> **Bug fix:** The existing code uses `saturating_sub(2)` for gap calculation,
> but `n` children have `n-1` gaps. This is fixed below to `saturating_sub(1)`.

```rust
Layout::Vertical => {
    let len = container.children.len();
    let total_weight: f64 = container.weights.iter().sum();
    let inner_gap = 1u16;
    let total_gap = inner_gap * (len as u16).saturating_sub(1);
    let used_area = (area.width.saturating_sub(total_gap)) as f64;
    let mut child_x = area.x;

    for (i, (child, &weight)) in container.children.iter()
        .zip(container.weights.iter()).enumerate()
    {
        let width = if i == len - 1 {
            container.area.x + container.area.width - child_x
        } else {
            ((weight / total_weight) * used_area).round() as u16
        };
        let child_area = Rect::new(
            child_x, container.area.y,
            width, container.area.height,
        );
        child_x += width + inner_gap;
        self.stack.push((*child, child_area));
    }
}
```

### Step 2.2: Minimum size enforcement

```rust
const MIN_SPLIT_WIDTH: u16 = 10;
const MIN_SPLIT_HEIGHT: u16 = 3;
```

After computing each child's dimension, clamp to the minimum. If a child would
be smaller, set it to the minimum and reduce remaining space.

---

## Phase 3: Resize API

### Step 3.1: `Tree::resize_view()`

**File:** `helix-view/src/tree.rs`

```rust
impl Tree {
    /// Resize the focused view by transferring weight to/from an adjacent
    /// sibling. `direction` determines which axis and direction to resize:
    /// - Right/Down: grow the focused view
    /// - Left/Up: shrink the focused view
    pub fn resize_view(&mut self, direction: Direction, amount: f64) {
        let focus = self.focus;
        let target_layout = match direction {
            Direction::Left | Direction::Right => Layout::Vertical,
            Direction::Up | Direction::Down => Layout::Horizontal,
        };
        let grows = matches!(direction, Direction::Right | Direction::Down);

        // Walk up the tree to find nearest ancestor with matching layout
        let mut current = focus;
        loop {
            let parent_id = self.nodes[current].parent;
            if parent_id == current {
                return; // reached root
            }
            let container = match &self.nodes[parent_id].content {
                Content::Container(c) => c,
                _ => unreachable!(),
            };
            if container.layout == target_layout {
                let pos = container.children.iter()
                    .position(|&id| id == current)
                    .unwrap();

                // Find adjacent sibling
                let sibling_pos = if grows {
                    if pos + 1 < container.children.len() {
                        pos + 1
                    } else {
                        return;
                    }
                } else {
                    if pos > 0 { pos - 1 } else { return; }
                };

                // Transfer weight
                let container = self.container_mut(parent_id);
                let max_transfer = container.weights[sibling_pos] - 0.1;
                let transfer = amount.min(max_transfer);
                if transfer <= 0.0 {
                    return;
                }
                container.weights[pos] += transfer;
                container.weights[sibling_pos] -= transfer;

                self.recalculate();
                return;
            }
            current = parent_id;
        }
    }
}
```

### Step 3.2: `Tree::equalize_splits()`

```rust
impl Tree {
    pub fn equalize_splits(&mut self) {
        self.equalize_recursive(self.root);
        self.recalculate();
    }

    fn equalize_recursive(&mut self, node_id: ViewId) {
        if let Content::Container(container) =
            &mut self.nodes[node_id].content
        {
            let len = container.children.len();
            container.weights = vec![1.0; len];
            let children: Vec<ViewId> = container.children.clone();
            for child in children {
                self.equalize_recursive(child);
            }
        }
    }
}
```

---

## Phase 4: Zoom / Unzoom

### Design

Zoom temporarily maximizes the focused split by inflating its weight relative
to siblings at every level of the tree. Unzoom restores the original weights.
This approach avoids hiding views from the tree (which would break focus
traversal, LSP diagnostics gutter, etc.) — all splits remain visible, just
very small.

The existing commented-out `fullscreen: bool` on `Tree` (line 11, 99) confirms
this was a planned feature.

### Step 4.1: Add zoom state to `Tree`

**File:** `helix-view/src/tree.rs`

```rust
pub struct Tree {
    // ... existing fields ...
    /// When Some, the focused view is zoomed. Stores the pre-zoom weights
    /// for every container so unzoom can restore them exactly.
    zoom_state: Option<ZoomState>,
}

struct ZoomState {
    zoomed_view: ViewId,
    /// Saved weights keyed by container ViewId.
    saved_weights: HashMap<ViewId, Vec<f64>>,
}
```

### Step 4.2: `Tree::zoom()` and `Tree::unzoom()`

```rust
impl Tree {
    pub fn toggle_zoom(&mut self) {
        if self.zoom_state.is_some() {
            self.unzoom();
        } else {
            self.zoom();
        }
    }

    fn zoom(&mut self) {
        let focus = self.focus;
        let mut saved_weights = HashMap::new();

        // Walk from focused view up to root, inflating the focused
        // child's weight at each container level.
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

            // Save original weights before modification
            saved_weights.insert(parent_id, container.weights.clone());

            let pos = container.children.iter()
                .position(|&id| id == current)
                .unwrap();

            // Set zoomed child to a large weight, others to minimum
            let container = self.container_mut(parent_id);
            let zoom_ratio = 10.0;
            for (i, w) in container.weights.iter_mut().enumerate() {
                if i == pos {
                    *w = zoom_ratio;
                } else {
                    *w = 0.1;
                }
            }

            current = parent_id;
        }

        self.zoom_state = Some(ZoomState {
            zoomed_view: focus,
            saved_weights,
        });

        self.recalculate();
    }

    fn unzoom(&mut self) {
        let state = match self.zoom_state.take() {
            Some(s) => s,
            None => return,
        };

        // Restore all saved weights
        for (container_id, weights) in state.saved_weights {
            if let Some(node) = self.nodes.get_mut(container_id) {
                if let Content::Container(container) = &mut node.content {
                    container.weights = weights;
                }
            }
        }

        self.recalculate();
    }

    pub fn is_zoomed(&self) -> bool {
        self.zoom_state.is_some()
    }
}
```

### Step 4.3: Auto-unzoom on split operations

When the user splits, closes a view, or changes focus while zoomed, auto-unzoom
to avoid confusing states:

- `Tree::split()`: call `self.unzoom()` at the start
- `Tree::remove()`: call `self.unzoom()` at the start
- `Tree::focus()` (focus change): call `self.unzoom()` if the new focus differs
  from `zoom_state.zoomed_view`

Alternatively, only auto-unzoom on structural changes (split/remove) and allow
focus to stay zoomed — this is a UX preference. tmux auto-unzooms on structural
changes but keeps zoom when switching panes within the zoomed layout.

---

## Phase 5: Commands and Keybindings

### Step 5.1: Command functions

**File:** `helix-term/src/commands.rs`

```rust
fn grow_width(cx: &mut Context) {
    let count = cx.count() as f64 * 0.2;
    cx.editor.tree.resize_view(tree::Direction::Right, count);
}

fn shrink_width(cx: &mut Context) {
    let count = cx.count() as f64 * 0.2;
    cx.editor.tree.resize_view(tree::Direction::Left, count);
}

fn grow_height(cx: &mut Context) {
    let count = cx.count() as f64 * 0.2;
    cx.editor.tree.resize_view(tree::Direction::Down, count);
}

fn shrink_height(cx: &mut Context) {
    let count = cx.count() as f64 * 0.2;
    cx.editor.tree.resize_view(tree::Direction::Up, count);
}

fn equalize_splits(cx: &mut Context) {
    cx.editor.tree.equalize_splits();
}

fn toggle_zoom(cx: &mut Context) {
    cx.editor.tree.toggle_zoom();
}
```

Register in `static_commands!`:

```rust
grow_width, "Grow split width",
shrink_width, "Shrink split width",
grow_height, "Grow split height",
shrink_height, "Shrink split height",
equalize_splits, "Equalize split sizes",
toggle_zoom, "Toggle zoom on focused split",
```

### Step 5.2: Default keybindings

**File:** `helix-term/src/keymap/default.rs`

In the `"C-w"` window mode section (after swap bindings ~line 213):

```rust
">" => grow_width,
"<" => shrink_width,
"+" => grow_height,
"-" => shrink_height,
"=" => equalize_splits,
"z" => toggle_zoom,
```

Same bindings in the select-mode window section (~line 260).

> `C-w z` matches tmux's `C-b z` zoom convention.

### Step 5.3: Typed commands

**File:** `helix-term/src/commands/typed.rs`

```rust
TypableCommand {
    name: "equalize-splits",
    aliases: &["equal"],
    doc: "Reset all splits to equal sizes.",
    fun: |cx, _args, event| {
        if event != PromptEvent::Validate { return Ok(()); }
        cx.editor.tree.equalize_splits();
        Ok(())
    },
    completer: CommandCompleter::none(),
    signature: Signature::none(),
},
TypableCommand {
    name: "toggle-zoom",
    aliases: &["zoom"],
    doc: "Toggle zoom on the focused split.",
    fun: |cx, _args, event| {
        if event != PromptEvent::Validate { return Ok(()); }
        cx.editor.tree.toggle_zoom();
        Ok(())
    },
    completer: CommandCompleter::none(),
    signature: Signature::none(),
},
```

---

## Phase 6: Mouse Drag Resizing (Optional)

### Step 6.1: Add drag state to `EditorView`

**File:** `helix-term/src/ui/editor.rs`

```rust
pub struct EditorView {
    // ... existing fields ...
    /// Active split border drag: (parent container ID, child index, layout)
    split_drag: Option<(ViewId, usize, Layout)>,
}
```

### Step 6.2: Detect split border on MouseDown

Add a helper to `Tree`:

```rust
pub fn find_split_border_at(&self, col: u16, row: u16)
    -> Option<(ViewId, usize)>
```

For **vertical** splits, the 1px `inner_gap` already exists — detect clicks
within that gap column.

For **horizontal** splits, there is no gap in the current layout. Two options:
- **Option A:** Add a 1px gap for horizontal layouts (consistent, but costs a
  row per split).
- **Option B:** Use a +/- 1 row hit zone around the boundary between children
  (no layout cost, slightly fuzzier detection).

Recommend **Option A** for consistency — add `inner_gap` to horizontal layout
in `recalculate()` as well.

### Step 6.3: Handle drag

On `MouseEventKind::Drag(MouseButton::Left)`: if `split_drag` is active,
compute new position relative to the container, adjust weights proportionally,
call `recalculate()`.

### Step 6.4: Clear on MouseUp

On `MouseUp(Left)`: set `split_drag = None`.

---

## Phase 7: Tests

**File:** `helix-view/src/tree.rs` (test module ~line 726)

- Verify existing `all_vertical_views_have_same_width` still passes (all
  weights initialized to 1.0 = equal distribution).
- Test `resize_view` changes weights and produces different-sized areas.
- Test `equalize_splits` resets to equal sizes.
- Test minimum size enforcement.
- Test removing a view preserves sibling weights.
- Test nested containers (e.g., vertical inside horizontal).
- Test that terminal resize preserves proportions.
- Test `toggle_zoom` inflates focused view and `toggle_zoom` again restores
  original weights exactly.
- Test zoom + close view: unzooms first, then removes correctly.
- Test zoom + split: unzooms first, then splits correctly.

---

## Edge Cases

| Case | Handling |
|------|----------|
| Single view | `resize_view` and `zoom` are no-ops (no sibling) |
| Nested splits | Parent-walk finds correct container layout |
| Terminal resize | `recalculate()` re-applies same ratios to new area |
| Closing a view | Weight removed with child; if container collapses to single child and merges up, remaining child inherits position in grandparent |
| Weight precision | `f64` is more than sufficient for ratios |
| All weight to one side | Minimum weight of 0.1 prevents complete collapse |
| Zoom + resize | Resizing while zoomed modifies the zoomed weights; unzoom restores pre-zoom weights (resize changes lost — acceptable) |
| Zoom + structural change | Auto-unzoom on split/remove prevents stale `saved_weights` referencing deleted containers |
| Double zoom | `toggle_zoom` when already zoomed just unzooms |

---

## Files Summary

| File | Action | Phase |
|------|--------|-------|
| `helix-view/src/tree.rs` | Add `weights`, modify `recalculate()`, add `resize_view()`, `equalize_splits()`, `toggle_zoom()`, `ZoomState` | 1-4 |
| `helix-term/src/commands.rs` | Add resize + equalize + zoom commands | 5 |
| `helix-term/src/keymap/default.rs` | Bind `> < + - = z` in `C-w` mode | 5 |
| `helix-term/src/commands/typed.rs` | Add `:equalize-splits`, `:toggle-zoom` | 5 |
| `helix-term/src/ui/editor.rs` | Mouse drag support | 6 |
| `book/src/keymap.md` | Document keybindings | 7 |
