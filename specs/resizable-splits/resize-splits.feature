@splits:resize
Feature: Resize editor splits

  Alex the developer works with multiple editor splits open simultaneously.
  To manage screen real estate, Alex can grow or shrink any split along
  either axis, with the adjacent sibling absorbing the difference.

  Background:
    Given Alex has opened Helix editor

  Rule: Growing a split increases its size at the expense of the adjacent sibling

    Example: Growing width of a side-by-side split
      Given Alex has two vertical splits open side by side with equal widths
      When Alex presses "C-w >" to grow the focused split's width
      Then the focused split should be wider than before
      And the adjacent sibling split should be narrower than before

    Example: Shrinking width transfers space to the adjacent sibling
      Given Alex has two vertical splits open side by side with equal widths
      When Alex presses "C-w <" to shrink the focused split's width
      Then the focused split should be narrower than before
      And the adjacent sibling split should be wider than before

    Example: Growing height of a stacked split
      Given Alex has two horizontal splits stacked vertically with equal heights
      When Alex presses "C-w +" to grow the focused split's height
      Then the focused split should be taller than before
      And the sibling split below should be shorter than before

    Example: Shrinking height of a stacked split
      Given Alex has two horizontal splits stacked vertically with equal heights
      When Alex presses "C-w -" to shrink the focused split's height
      Then the focused split should be shorter than before
      And the sibling split below should be taller than before

  Rule: Resize amount scales with a count prefix

    Example: A count of 3 applies three times the default resize step
      Given Alex has two vertical splits open side by side with equal widths
      When Alex presses "3 C-w >" to grow the focused split
      Then the width gained should be three times the width gained by a single "C-w >"

    Scenario Outline: Count prefix multiplies the resize step proportionally
      Given Alex has two vertical splits with equal widths
      When Alex presses "<count> C-w >" to grow width
      Then the focused split should gain "<count>" resize steps of width

      Examples:
        | count |
        | 1     |
        | 2     |
        | 5     |

  Rule: Resize walks up the tree to find the nearest ancestor with matching layout axis

    Example: Growing width when the focused split is nested inside a horizontal container
      Given Alex has a horizontal split where the top half contains two vertical sub-splits
      And focus is on the left sub-split within the top half
      When Alex presses "C-w >" to grow width
      Then the left sub-split grows at the expense of the right sub-split in the top half
      And the top-level horizontal split proportions are unaffected

    Example: Growing height of a split nested inside a vertical container bubbles up
      Given Alex has a horizontal split where the top half contains two vertical sub-splits
      And focus is on the left sub-split within the top half
      When Alex presses "C-w +" to grow height
      Then the top half grows at the expense of the bottom half at the outer level
      And the vertical sub-split proportions within the top half are unaffected

  Rule: Resize is a no-op when the focused split has no sibling in the requested direction

    Example: Cannot grow width of the rightmost split
      Given Alex has two vertical splits and focus is on the rightmost split
      When Alex presses "C-w >" to grow width
      Then the split sizes remain unchanged

    Example: Cannot shrink width of the leftmost split
      Given Alex has two vertical splits and focus is on the leftmost split
      When Alex presses "C-w <" to shrink width
      Then the split sizes remain unchanged

    Example: Resize on a single-split layout does nothing
      Given Alex has only one split open
      When Alex presses "C-w >" to grow width
      Then the split size remains unchanged and no error occurs

  Rule: Minimum weight prevents a sibling from collapsing completely

    Example: Aggressively growing width stops before the sibling reaches minimum weight
      Given Alex has two vertical splits where the right split is already very narrow
      When Alex presses "C-w >" with an amount that would collapse the right split
      Then the right split retains a minimum visible width
      And the focused split has grown as much as possible without collapsing the sibling
