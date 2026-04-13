@modal-picker:modes
Feature: Modal picker mode switching

  The ModalPicker is a reusable UI component that any command (buffer list,
  file finder, search results, etc.) can instantiate. It has two modes:
  Insert for typing a query, Normal for navigating and acting on items.
  Callers of the component never need to implement mode logic themselves.

  Rule: The picker opens in Insert mode, ready to filter immediately

    Example: A newly opened picker accepts typed characters as search input
      Given Alex opens a picker built on the ModalPicker component
      When Alex types "main"
      Then the item list is filtered to entries matching "main"
      And the mode indicator shows "INSERT"

    Example: The prompt cursor is active in Insert mode
      Given the picker is in Insert mode
      Then the query field shows a text cursor

  Rule: Pressing Escape in Insert mode switches to Normal mode

    Example: Escape from Insert mode enters Normal mode
      Given the picker is in Insert mode
      When Alex presses "<Esc>"
      Then the picker is in Normal mode
      And the mode indicator shows "NORMAL"
      And the query text is unchanged

  Rule: Pressing Escape in Normal mode with no selection closes the picker

    Example: Escape from Normal mode with nothing selected closes the picker
      Given the picker is in Normal mode
      And no items are selected
      When Alex presses "<Esc>"
      Then the picker is closed without performing any action

  Rule: Pressing Escape in Normal mode with active selections clears them first

    Example: First Escape in Normal mode clears selections without closing
      Given the picker is in Normal mode
      And two items are selected
      When Alex presses "<Esc>"
      Then no items are selected
      And the picker remains open

    Example: Second Escape in Normal mode closes the picker
      Given the picker is in Normal mode
      And no items are selected
      When Alex presses "<Esc>"
      Then the picker is closed

  Rule: Printable characters and "/" in Normal mode re-enter Insert mode

    Example: Pressing "i" in Normal mode enters Insert mode
      Given the picker is in Normal mode
      When Alex presses "i"
      Then the picker is in Insert mode
      And the mode indicator shows "INSERT"

    Example: Pressing "/" in Normal mode enters Insert mode
      Given the picker is in Normal mode
      When Alex presses "/"
      Then the picker is in Insert mode

    Example: An unbound printable key in Normal mode enters Insert mode and appends the character
      Given the picker is in Normal mode
      And "a" is not bound to a Normal mode action in this picker instance
      When Alex presses "a"
      Then the picker is in Insert mode
      And the query field ends with "a"

  Rule: The current mode is always visible in the picker UI

    Example: Mode indicator updates immediately on each mode transition
      Given the picker is in Insert mode
      When Alex presses "<Esc>"
      Then the mode indicator shows "NORMAL"
      When Alex presses "i"
      Then the mode indicator shows "INSERT"
