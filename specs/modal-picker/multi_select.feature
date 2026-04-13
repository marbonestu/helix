@modal-picker:multi-select
Feature: Modal picker multi-select

  In Normal mode Alex the developer can mark multiple items before acting on
  them all at once. Multi-select is built into the ModalPicker component —
  callers register bulk actions and the component routes them automatically
  to the full selection set when items are selected, or to the cursor item
  when nothing is selected.

  Background:
    Given Alex opens a picker with 5 items in the list
    And the picker is in Normal mode
    And no items are selected

  Rule: Space in Normal mode toggles the selection of the item under the cursor

    Example: Pressing Space selects an unselected item and advances the cursor
      Given the cursor is on item 2
      When Alex presses "<Space>"
      Then item 2 is marked as selected
      And the selection count shows 1 item selected
      And the cursor moves to item 3

    Example: Pressing Space on a selected item deselects it
      Given item 2 is marked as selected
      And the cursor is on item 2
      When Alex presses "<Space>"
      Then item 2 is not marked as selected
      And the selection count shows 0 items selected

    Example: Space-selecting down the list builds a multi-item selection
      Given the cursor is on item 1
      When Alex presses "<Space>", "j", "<Space>", "j", "<Space>"
      Then items 1, 2, and 3 are marked as selected
      And the selection count shows 3 items selected

  Rule: "%" selects all currently visible items; pressing it again deselects them

    Example: "%" selects all items visible after the current filter
      Given the query filters the list to 3 matching items
      When Alex presses "%"
      Then all 3 visible items are marked as selected

    Example: "%" only selects filtered results, not items hidden by the query
      Given the list has 5 total items
      And the query filters the list to 2 matching items
      When Alex presses "%"
      Then the selection count shows 2 items selected

    Example: Pressing "%" a second time deselects all visible items
      Given all 3 visible items are marked as selected
      When Alex presses "%"
      Then no items are selected

  Rule: Selections survive query changes — they are tracked by item identity, not position

    Example: A selected item remains selected even when filtered out of view
      Given item 2 is marked as selected
      And the picker is in Insert mode
      When Alex updates the query so that item 2 is no longer visible
      Then item 2 remains in the selection set
      And the selection count still shows 1 item selected

    Example: A previously hidden selected item reappears as selected when the filter is cleared
      Given item 2 is marked as selected
      And the query has filtered item 2 out of view
      When Alex clears the query
      Then item 2 reappears with the selected indicator visible

  Rule: A registered bulk action receives all selected items; without a selection it receives the cursor item

    Example: Invoking a bulk action with a selection passes all selected items to the action
      Given items 2, 3, and 4 are marked as selected
      When Alex invokes the picker's registered bulk action
      Then the action receives items 2, 3, and 4

    Example: Invoking a bulk action with no selection passes only the cursor item
      Given no items are selected
      And the cursor is on item 3
      When Alex invokes the picker's registered bulk action
      Then the action receives only item 3

  Rule: Selected items are visually distinct from the cursor item and unselected items

    Example: Selected items, the cursor item, and unselected items each render differently
      Given items 1 and 3 are marked as selected
      And the cursor is on item 2
      Then item 1 displays the selected indicator
      And item 3 displays the selected indicator
      And item 2 displays the cursor highlight only
      And item 4 displays no indicator

  Rule: The selection count is shown in the picker status area

    Example: The status area reflects the current selection count
      Given 3 items are marked as selected
      Then the status area shows "3 selected"

    Example: The status area is empty when nothing is selected
      Given no items are selected
      Then the status area does not show a selection count
