@session-persistence @full-scope
Feature: Full scope session persistence

  Alex the developer relies on named registers for macros, yanked text, and
  search patterns. With scope set to "full", Helix persists this editor state
  alongside the layout so Alex's registers and jump history survive restarts.

  Background:
    Given Alex has configured "scope = \"full\"" in "[editor.session]"
    And Alex has Helix open in a project directory

  Rule: Named registers are captured and restored under full scope

    Example: A register saved before exit is available after restore
      Given Alex has yanked "hello world" into register "a"
      When Alex saves the session and reopens Helix
      Then pasting from register "a" produces "hello world"

    Example: The search register is restored so the last search pattern is available
      Given Alex has searched for "fn capture" leaving it in the search register
      When Alex saves the session and reopens Helix
      Then the search register contains "fn capture"

    Example: The yank register is restored after a session save and restore
      Given Alex has yanked a line into the default yank register
      When Alex saves the session and reopens Helix
      Then the default yank register still contains the yanked line

  Rule: Read-only and system registers are never persisted

    Example: System clipboard registers are not included in the session file
      Given Alex has text in the system clipboard register "*"
      When Alex saves the session
      Then the session file does not contain a clipboard register entry

    Scenario Outline: Each read-only register is excluded from the session
      Given Alex has content in the "<register>" register
      When Alex saves the session
      Then the session file does not contain an entry for "<register>"

      Examples:
        | register |
        | _        |
        | #        |
        | .        |
        | %        |
        | *        |
        | +        |

  Rule: Jump list history is preserved per view under full scope

    Example: Jump list entries survive a session save and restore
      Given Alex has navigated to several locations in "/src/main.rs" building up a jump history
      When Alex saves the session and reopens Helix
      Then Alex can navigate backwards through the same jump history

  Rule: Registers are not persisted when scope is "layout"

    Example: Register contents are not included in the session when scope is layout
      Given Alex has configured "scope = \"layout\"" in "[editor.session]"
      And Alex has yanked "important text" into register "a"
      When Alex saves the session
      Then the session file does not contain any register entries
