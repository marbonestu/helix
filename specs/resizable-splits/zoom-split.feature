@splits:zoom
Feature: Zoom focused split

  Alex the developer wants to temporarily maximize the active editor split
  to focus deeply on a file without closing other splits. This follows the
  tmux C-b z convention and can be toggled off to restore the prior layout.

  Background:
    Given Alex has opened Helix editor

  Rule: Zooming maximizes the focused split while keeping all splits visible

    Example: Zooming one of two side-by-side splits
      Given Alex has two vertical splits open with equal widths
      When Alex presses "C-w z" to zoom the focused split
      Then the focused split dominates most of the available width
      And the other split is still visible but very narrow
      And the editor indicates that a split is currently zoomed

    Example: Zooming a nested split maximizes it at every container level
      Given Alex has a horizontal split where the top half contains two vertical sub-splits
      And focus is on the left sub-split within the top half
      When Alex presses "C-w z" to zoom
      Then the left sub-split dominates the top half
      And the top half dominates the full editor area

    Example: All splits remain open and reachable while zoomed
      Given Alex has three vertical splits each showing a different file
      When Alex zooms the middle split
      Then all three splits are still open in the editor
      And Alex can navigate focus to the other splits

  Rule: Unzooming restores the exact pre-zoom layout

    Example: Toggling zoom twice returns to the original proportions
      Given Alex has two vertical splits where the left occupies two-thirds of the width
      When Alex presses "C-w z" to zoom and then "C-w z" again to unzoom
      Then the left split occupies two-thirds of the width again

    Example: Pre-zoom sizes are restored exactly regardless of how unequal they were
      Given Alex has three vertical splits with widths in a 5:2:1 ratio
      When Alex zooms the first split and then unzooms
      Then the three splits return to their 5:2:1 width ratio

    Example: Zoom toggled via the ":toggle-zoom" typed command
      Given Alex has two vertical splits with unequal widths
      When Alex runs ":toggle-zoom" to zoom and then ":toggle-zoom" again to unzoom
      Then both splits return to their original widths

    Example: Zoom toggled via the ":zoom" alias
      Given Alex has two vertical splits with unequal widths
      When Alex runs ":zoom" to zoom and then ":zoom" again to unzoom
      Then both splits return to their original widths

  Rule: Creating a new split while zoomed auto-unzooms first

    Example: Splitting a zoomed view clears zoom before adding the new split
      Given Alex has two vertical splits and the left split is zoomed
      When Alex creates a new vertical split with "C-w v"
      Then the zoom state is cleared
      And the editor now has three splits

  Rule: Closing a split while zoomed auto-unzooms first

    Example: Closing a split when zoomed leaves a clean unzoomed layout
      Given Alex has two vertical splits and the left split is zoomed
      When Alex closes the right split with ":q"
      Then the zoom state is cleared
      And the remaining split occupies the full editor area

  Rule: Zooming with only one split open is a no-op

    Example: Zoom on a single-split layout has no visible effect
      Given Alex has only one split open
      When Alex presses "C-w z" to zoom
      Then the single split still occupies the full editor area
      And no error occurs
