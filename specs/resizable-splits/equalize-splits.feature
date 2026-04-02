@splits:equalize
Feature: Equalize split sizes

  After resizing splits for a specific task, Alex the developer wants to
  quickly restore a balanced layout without manually adjusting each split.
  Equalize resets all weights to equal distribution throughout the tree.

  Background:
    Given Alex has opened Helix editor with multiple splits

  Rule: Equalize resets all splits to equal sizes

    Example: Two unequal vertical splits equalized via keybinding
      Given Alex has two vertical splits where the left is twice as wide as the right
      When Alex presses "C-w =" to equalize splits
      Then both splits have equal widths

    Example: Equalized splits differ by at most one column due to rounding
      Given Alex has three vertical splits with unequal widths in a terminal that is not divisible by three
      When Alex presses "C-w =" to equalize splits
      Then the widths of all three splits differ by at most 1 column

    Example: Equalizing via the typed command ":equalize-splits"
      Given Alex has two vertical splits with unequal widths
      When Alex runs ":equalize-splits"
      Then both splits have equal widths

    Example: Equalizing via the ":equal" alias
      Given Alex has two vertical splits with unequal widths
      When Alex runs ":equal"
      Then both splits have equal widths

  Rule: Equalize applies recursively to all nested containers

    Example: Nested splits at all levels are equalized in one operation
      Given Alex has a vertical split where the right half contains two stacked horizontal splits
      And the outer vertical split has a 3:1 width ratio
      And the inner horizontal splits have a 4:1 height ratio
      When Alex presses "C-w =" to equalize splits
      Then the left and right outer halves have equal widths
      And the two horizontal sub-splits within the right half have equal heights

  Rule: Equalizing an already-equal layout is idempotent

    Example: Pressing equalize on equal splits leaves them equal
      Given Alex has three vertical splits with equal widths
      When Alex presses "C-w =" to equalize
      Then all three splits still have equal widths
