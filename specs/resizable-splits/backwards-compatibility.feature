@splits:compatibility
Feature: Backwards compatible split behavior

  The introduction of weighted splits must not change the default behavior
  that Alex the developer and existing users rely on. With no explicit
  resizing, splits should look and behave exactly as they did before.

  Rule: Default splits distribute space equally, matching pre-feature behavior

    Example: Two new vertical splits have equal widths by default
      Given Alex opens Helix and creates a vertical split
      Then both splits have exactly equal widths as they always have

    Example: Three splits created in sequence have equal widths
      Given Alex creates three vertical splits in sequence without resizing
      Then all three splits have equal widths

    Example: Two horizontal splits created in sequence have equal heights
      Given Alex creates two horizontal splits in sequence without resizing
      Then both splits have equal heights

  Rule: Existing split workflows are unaffected

    Example: Write-quit in one split closes it while the other remains open
      Given Alex has two splits open showing the same file
      When Alex saves and quits from the focused split
      Then the other split remains open with the saved file

    Example: Edits in one split are reflected in all splits showing the same document
      Given Alex has two vertical splits open showing the same file
      When Alex edits text in the focused split
      Then the change is visible in both splits
