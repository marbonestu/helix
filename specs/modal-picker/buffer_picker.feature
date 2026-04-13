@modal-picker:buffer-picker
Feature: Buffer picker built on ModalPicker

  The buffer picker is an instance of the ModalPicker component. It lists
  open buffers and lets Alex the developer close one or many at once using
  the multi-select workflow. Buffer-specific rules (unsaved confirmation,
  active buffer handling) live here; generic picker rules live in the
  ModalPicker feature files.

  Background:
    Given Alex has several buffers open
    And Alex opens the buffer picker

  Rule: With no selection, "d" in Normal mode closes the buffer under the cursor

    Example: "d" with no selection closes the buffer at the cursor and refreshes the list
      Given the picker is in Normal mode
      And no buffers are selected
      And the cursor is on "foo.rs"
      When Alex presses "d"
      Then "foo.rs" is closed
      And the picker refreshes without "foo.rs" in the list

  Rule: With a selection, "d" in Normal mode closes all selected buffers

    Example: "d" closes every selected buffer and refreshes the list
      Given "foo.rs", "bar.rs", and "baz.rs" are marked as selected
      When Alex presses "d"
      Then "foo.rs", "bar.rs", and "baz.rs" are all closed
      And the picker refreshes showing only the remaining open buffers
      And no items are marked as selected

    Example: The buffer under the cursor is not closed unless it is also selected
      Given "foo.rs" and "bar.rs" are marked as selected
      And the cursor is on "qux.rs"
      When Alex presses "d"
      Then "foo.rs" and "bar.rs" are closed
      And "qux.rs" remains open

  Rule: Closing an unsaved buffer requires explicit confirmation

    Example: "d" on an unsaved buffer shows a confirmation prompt
      Given "unsaved.rs" has unsaved changes
      And no buffers are selected
      And the cursor is on "unsaved.rs"
      When Alex presses "d"
      Then a confirmation prompt asks whether to discard changes in "unsaved.rs"

    Example: Confirming the prompt closes the unsaved buffer
      Given a confirmation prompt is shown for "unsaved.rs"
      When Alex confirms
      Then "unsaved.rs" is closed without saving
      And the picker refreshes

    Example: Cancelling the prompt leaves the buffer open
      Given a confirmation prompt is shown for "unsaved.rs"
      When Alex cancels
      Then "unsaved.rs" remains open
      And the picker is still visible with no change to the list

    Example: Bulk delete prompts separately for each unsaved buffer in the selection
      Given "clean.rs" has no unsaved changes
      And "dirty.rs" has unsaved changes
      And both are marked as selected
      When Alex presses "d"
      Then "clean.rs" is closed immediately without prompting
      And a confirmation prompt is shown for "dirty.rs"

  Rule: Closing the currently active buffer leaves the editor in a valid state

    Example: Closing the active buffer switches focus to the next available buffer
      Given "active.rs" is the currently focused buffer in the editor
      And "active.rs" and "other.rs" are marked as selected
      When Alex presses "d"
      Then both buffers are closed
      And the editor focuses on a remaining open buffer

    Example: Closing the only open buffer opens a new empty scratch buffer
      Given "only.rs" is the only open buffer
      And the cursor is on "only.rs"
      And no buffers are selected
      When Alex presses "d"
      Then "only.rs" is closed
      And the editor opens a new empty scratch buffer

  Rule: The buffer picker stays open and refreshes after every delete operation

    Example: After a bulk delete the picker lists only the surviving buffers
      Given "foo.rs" and "bar.rs" are marked as selected
      And "qux.rs" is open but not selected
      When Alex presses "d"
      Then the picker remains open
      And the list shows only "qux.rs"
      And no items are marked as selected
