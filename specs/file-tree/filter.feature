@file-tree @navigation
Feature: Filter-as-you-type to narrow the visible tree

  Alex the developer can type a query while the file tree is focused to instantly
  narrow the visible rows to only entries whose names contain the query string.
  Unlike incremental search, filtering hides non-matching rows rather than jumping
  between them, making it easier to focus on a subset of a large tree.

  Background:
    Given the file tree sidebar is visible and focused
    And the project contains the structure:
      """
      project/
        src/
          main.rs
          lib.rs
          utils.rs
          models/
            user.rs
            post.rs
        tests/
          integration.rs
          unit.rs
        Cargo.toml
        README.md
      """

  Rule: f enters filter mode and shows a prompt

    Example: Pressing f activates the filter prompt
      Given no filter is active
      When Alex presses f
      Then a filter prompt appears at the bottom of the sidebar
      And all tree rows remain visible with an empty query

    Example: Typing characters appends them to the filter query
      Given filter mode is active with an empty query
      When Alex types "rs"
      Then the prompt displays "rs"

  Rule: Only rows whose name matches the query remain visible

    Example: Typing narrows the tree to matching filenames
      Given filter mode is active
      When Alex types "rs"
      Then only main.rs, lib.rs, utils.rs, user.rs, post.rs, integration.rs, and unit.rs are visible
      And Cargo.toml and README.md are hidden

    Example: Matching is case-insensitive
      Given filter mode is active
      When Alex types "README"
      Then README.md is visible

    Example: Parent directories of matching files remain visible
      Given filter mode is active
      When Alex types "user"
      Then user.rs is visible
      And src/ and models/ remain visible as ancestor context
      And unmatched siblings like post.rs are hidden

    Example: Backspace broadens the filter
      Given filter mode is active with query "user"
      And only user.rs and its ancestors are visible
      When Alex presses Backspace
      Then the query becomes "use"
      And any additional entries matching "use" become visible

    Example: Clearing the query restores all rows
      Given filter mode is active with query "rs"
      When Alex clears the query with Backspace until empty
      Then all rows are visible again

  Rule: Escape clears the filter and restores the full tree

    Example: Escape exits filter mode and shows all entries
      Given filter mode is active with query "rs"
      And several rows are hidden
      When Alex presses Escape
      Then filter mode is no longer active
      And all rows are visible again
      And the filter prompt disappears

  Rule: Enter confirms the filter and returns to normal mode with the filtered view

    Example: Enter keeps the narrowed view active after exiting filter mode
      Given filter mode is active with query "test"
      And only test-related files are visible
      When Alex presses Enter
      Then filter mode is no longer active
      And only the matching rows remain visible
      And Alex can navigate among them with j and k

  Rule: Navigation in filter mode moves only among visible rows

    Example: j skips hidden rows and moves to the next visible row
      Given filter mode is active with query "rs"
      And main.rs is selected
      When Alex presses j
      Then the selection moves to lib.rs (the next visible row)
      And Cargo.toml is skipped because it is hidden

  Rule: A query with no matches shows an empty tree with a status message

    Example: Unmatched query hides all rows and informs Alex
      Given filter mode is active
      When Alex types "zzz"
      Then no rows are visible in the tree
      And a message in the sidebar indicates no files match "zzz"
