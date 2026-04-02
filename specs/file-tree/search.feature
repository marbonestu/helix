@file-tree @search
Feature: Incremental search and jump within the file tree

  Alex the developer can type a query to jump instantly to any file or directory
  whose name contains the search string, without navigating row by row.

  Background:
    Given the file tree sidebar is visible and focused
    And the project tree contains nodes named:
      | name           | kind      |
      | src            | directory |
      | main.rs        | file      |
      | lib.rs         | file      |
      | tests          | directory |
      | integration.rs | file      |
      | Cargo.toml     | file      |
      | README.md      | file      |

  Rule: / enters search mode and shows a prompt

    Example: Pressing / activates the search prompt
      Given no search is active
      When Alex presses /
      Then a search prompt appears at the bottom of the sidebar
      And the prompt displays a leading slash with an empty query

    Example: Typing characters appends them to the query
      Given search mode is active with an empty query
      When Alex types "src"
      Then the prompt displays "/src"

  Rule: The selection tracks the first matching node as the query changes

    Example: Query immediately jumps to the first matching node
      Given search mode is active
      When Alex types "lib"
      Then the selection moves to lib.rs

    Example: Appending a character refines the match
      Given search mode is active with query "r"
      And the selection is on main.rs (first "r" match)
      When Alex types "s"
      Then the query becomes "rs"
      And the selection moves to the first node whose name contains "rs"

    Example: Backspace removes the last character and re-evaluates
      Given search mode is active with query "lib"
      And the selection is on lib.rs
      When Alex presses Backspace
      Then the query becomes "li"
      And the selection updates to the first match for "li"

    Example: Backspace on an empty query keeps search mode active
      Given search mode is active with an empty query
      When Alex presses Backspace
      Then the query remains empty
      And search mode stays active

  Rule: Matching is case-insensitive

    Scenario Outline: Query matches regardless of case
      Given search mode is active
      When Alex types "<query>"
      Then the selection lands on a node whose name contains "src" case-insensitively

      Examples:
        | query |
        | src   |
        | SRC   |
        | Src   |

  Rule: ctrl-n and ctrl-p cycle through matches while search mode is active

    Example: ctrl-n advances to the next matching node
      Given search mode is active with query "rs"
      And main.rs is currently selected (first match)
      When Alex presses ctrl-n
      Then the selection moves to lib.rs (second match)

    Example: ctrl-p moves to the previous matching node
      Given search mode is active with query "rs"
      And lib.rs is currently selected
      When Alex presses ctrl-p
      Then the selection moves back to main.rs

    Example: ctrl-n wraps around from the last match to the first
      Given search mode is active with query "rs"
      And integration.rs is currently selected (last match)
      When Alex presses ctrl-n
      Then the selection wraps to main.rs (first match)

    Example: ctrl-p wraps around from the first match to the last
      Given search mode is active with query "rs"
      And main.rs is currently selected (first match)
      When Alex presses ctrl-p
      Then the selection wraps to integration.rs (last match)

  Rule: Enter confirms the search and returns to normal mode

    Example: Enter keeps the current selection and exits search mode
      Given search mode is active with query "lib"
      And lib.rs is selected
      When Alex presses Enter
      Then search mode is no longer active
      And lib.rs remains selected
      And the search prompt disappears

  Rule: Escape cancels the search and restores the previous selection

    Example: Escape exits search mode and restores position
      Given the selection was on Cargo.toml before search started
      And search mode is active with query "lib"
      And lib.rs is selected
      When Alex presses Escape
      Then search mode is no longer active
      And the selection returns to Cargo.toml
      And the search prompt disappears

  Rule: n and N jump to next/previous match when not in search mode

    Example: n jumps to the next node matching the last query
      Given Alex previously searched for "rs" and confirmed with Enter
      And search mode is no longer active
      When Alex presses n
      Then the selection moves to the next node whose name contains "rs"

    Example: N jumps to the previous node matching the last query
      Given Alex previously searched for "rs" and confirmed with Enter
      And a node in the middle of the "rs" matches is selected
      When Alex presses N
      Then the selection moves to the previous node whose name contains "rs"

  Rule: A query with no matches does not move the selection

    Example: No-match query leaves the selection unchanged
      Given search mode is active
      And Cargo.toml is currently selected
      When Alex types "zzz"
      Then the selection stays on Cargo.toml
