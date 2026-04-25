@sidebar:resize
Feature: Resize the sidebar panel

  Alex the developer keeps the file-tree sidebar open while editing. To
  balance screen real estate between the sidebar and the editor splits,
  Alex can grow or shrink the sidebar width at runtime without restarting
  the editor. Key bindings work both when the sidebar is focused (direct
  sidebar resize) and when an editor split is focused (split resize that
  leaves the sidebar width untouched).

  Background:
    Given Alex has opened Helix editor

  Rule: C-right grows the sidebar by one column when the sidebar is focused

    Example: Pressing C-right once grows the sidebar width by 1
      Given the sidebar is open and focused
      And the sidebar width is 30 columns
      When Alex presses "<C-right>"
      Then the sidebar should be 31 columns wide

  Rule: C-left shrinks the sidebar by one column when the sidebar is focused

    Example: Pressing C-left once shrinks the sidebar width by 1
      Given the sidebar is open and focused
      And the sidebar width is 30 columns
      When Alex presses "<C-left>"
      Then the sidebar should be 29 columns wide

    Example: Shrinking stops at the minimum width of 5 columns
      Given the sidebar is open and focused
      And the sidebar width is 5 columns
      When Alex presses "<C-left>"
      Then the sidebar should be 5 columns wide

  Rule: C-w > and C-w < resize the sidebar when it is focused

    Example: C-w > grows the sidebar
      Given the sidebar is open and focused
      And the sidebar width is 30 columns
      When Alex presses "<C-w>>"
      Then the sidebar should be wider than 30 columns

    Example: C-w < shrinks the sidebar
      Given the sidebar is open and focused
      And the sidebar width is 30 columns
      When Alex presses "<C-w><"
      Then the sidebar should be narrower than 30 columns

  Rule: e toggles the sidebar between normal width and expanded (half-terminal) mode

    Example: Pressing e while sidebar is focused switches to expanded mode
      Given the sidebar is open and focused
      When Alex presses "e"
      Then the sidebar should be expanded

    Example: Pressing e a second time collapses back to normal width
      Given the sidebar is open and focused
      When Alex presses "e"
      And Alex presses "e"
      Then the sidebar should be collapsed

  Rule: A count prefix multiplies the resize step

    Example: 3 C-right grows the sidebar by 3 columns
      Given the sidebar is open and focused
      And the sidebar width is 30 columns
      When Alex presses "3<C-right>"
      Then the sidebar should be 33 columns wide

    Example: 3 C-left shrinks the sidebar by 3 columns
      Given the sidebar is open and focused
      And the sidebar width is 30 columns
      When Alex presses "3<C-left>"
      Then the sidebar should be 27 columns wide

  Rule: Typable commands resize the sidebar

    Example: :grow-sidebar grows the sidebar by 1 column
      Given the sidebar is open and focused
      And the sidebar width is 30 columns
      When Alex presses ":grow-sidebar<ret>"
      Then the sidebar should be 31 columns wide

    Example: :shrink-sidebar shrinks the sidebar by 1 column
      Given the sidebar is open and focused
      And the sidebar width is 30 columns
      When Alex presses ":shrink-sidebar<ret>"
      Then the sidebar should be 29 columns wide

    Example: :grow-sidebar accepts a count argument
      Given the sidebar is open and focused
      And the sidebar width is 30 columns
      When Alex presses ":grow-sidebar 5<ret>"
      Then the sidebar should be 35 columns wide

    Example: :shrink-sidebar stops at minimum width
      Given the sidebar is open and focused
      And the sidebar width is 5 columns
      When Alex presses ":shrink-sidebar<ret>"
      Then the sidebar should be 5 columns wide

  Rule: Resizing editor splits does not change the sidebar width

    Example: Growing a split when the sidebar is visible and not focused leaves sidebar width unchanged
      Given Alex has two vertical splits open side by side with equal widths
      And the sidebar is open but not focused
      And the sidebar width is 30 columns
      When Alex presses "<C-w>>" to grow the focused split's width
      Then the sidebar should be 30 columns wide
