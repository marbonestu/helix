@modal-picker:navigation
Feature: Modal picker Normal mode navigation

  In Normal mode Alex the developer navigates the item list using Vim-style
  motions. These motions are built into the ModalPicker component and work
  the same regardless of what kind of items the picker is showing — buffers,
  files, grep results, or anything else.

  Background:
    Given Alex opens a picker with 10 items in the list
    And the picker is in Normal mode

  Rule: j/k move the cursor one item down or up

    Example: Pressing "j" moves the cursor to the next item
      Given the cursor is on item 3
      When Alex presses "j"
      Then the cursor is on item 4

    Example: Pressing "k" moves the cursor to the previous item
      Given the cursor is on item 3
      When Alex presses "k"
      Then the cursor is on item 2

    Example: Pressing "j" on the last item wraps to the first
      Given the cursor is on the last item
      When Alex presses "j"
      Then the cursor is on item 1

    Example: Pressing "k" on the first item wraps to the last
      Given the cursor is on item 1
      When Alex presses "k"
      Then the cursor is on the last item

  Rule: "gg" jumps to the first item, "G" jumps to the last

    Example: "gg" moves the cursor to the first item from anywhere
      Given the cursor is on item 7
      When Alex presses "gg"
      Then the cursor is on item 1

    Example: "G" moves the cursor to the last item from anywhere
      Given the cursor is on item 2
      When Alex presses "G"
      Then the cursor is on the last item

  Rule: Ctrl+D and Ctrl+U scroll half a page down and up

    Example: Ctrl+D moves the cursor down by half the visible list height
      Given the cursor is on item 1
      And the visible list height is 10 items
      When Alex presses "<C-d>"
      Then the cursor is on item 6

    Example: Ctrl+U moves the cursor up by half the visible list height
      Given the cursor is on item 8
      And the visible list height is 10 items
      When Alex presses "<C-u>"
      Then the cursor is on item 3

  Rule: Enter in Normal mode invokes the default action on the item under the cursor

    Example: Enter accepts the item at the cursor and closes the picker
      Given the cursor is on item 3
      When Alex presses "<Enter>"
      Then the picker's default action is invoked with the item at position 3
      And the picker is closed

  Rule: Navigation in Normal mode does not modify the query

    Example: Moving the cursor does not change the search query
      Given the query contains "foo"
      And the cursor is on item 2
      When Alex presses "j"
      Then the query still contains "foo"
