@session-persistence @layout
Feature: Split layout persistence

  Alex the developer organises their work across multiple panes — a source
  file on the left, tests on the right, config at the bottom. Persisting the
  split layout means Alex never has to rebuild that arrangement from scratch.

  Background:
    Given Alex has configured "persist = true" in "[editor.session]"

  Rule: The number and direction of splits is preserved

    Example: A vertical split with two files is restored correctly
      Given Alex has a vertical split with "/src/main.rs" on the left and "/src/lib.rs" on the right
      When Alex saves the session and reopens Helix
      Then two panes are open side by side
      And the left pane shows "/src/main.rs"
      And the right pane shows "/src/lib.rs"

    Example: A horizontal split with two files is restored correctly
      Given Alex has a horizontal split with "/src/main.rs" on top and "/tests/main_test.rs" on the bottom
      When Alex saves the session and reopens Helix
      Then two panes are open stacked vertically
      And the top pane shows "/src/main.rs"
      And the bottom pane shows "/tests/main_test.rs"

    Example: A nested split layout is fully restored
      Given Alex has a vertical split where the right pane is itself split horizontally
      And the three panes contain "/src/main.rs", "/src/lib.rs", and "/tests/lib_test.rs"
      When Alex saves the session and reopens Helix
      Then three panes are open matching the saved arrangement

  Rule: Focus is restored to the pane that was active when the session was saved

    Example: The focused pane is active after restore
      Given Alex has a vertical split with "/src/main.rs" on the left and "/src/lib.rs" on the right
      And the right pane is focused
      When Alex saves the session and reopens Helix
      Then the right pane showing "/src/lib.rs" is the active pane

  Rule: The scroll position of each pane is preserved

    Example: Each pane is scrolled to the same position as when the session was saved
      Given Alex has "/src/main.rs" open scrolled to line 150
      And "/src/lib.rs" open in a second pane scrolled to line 80
      When Alex saves the session and reopens Helix
      Then the first pane is scrolled to line 150
      And the second pane is scrolled to line 80
