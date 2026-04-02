@splits:layout
Feature: Proportional split layout

  The editor represents split sizes as proportional weights so that
  resizing the terminal window preserves Alex the developer's intended
  layout rather than reverting to equal distribution.
  New splits always start at equal weight to avoid surprising jumps.

  Background:
    Given Alex has opened Helix editor

  Rule: Terminal resize preserves split proportions

    Example: Two unequal splits maintain their ratio after a terminal resize
      Given Alex has two vertical splits where the left occupies two-thirds of the width
      When Alex resizes the terminal window to a different size
      Then the left split still occupies approximately two-thirds of the new width
      And the right split occupies approximately one-third of the new width

    Example: Nested split proportions are preserved after a terminal resize
      Given Alex has a horizontal split where the top half contains two vertical sub-splits in a 2:1 ratio
      When Alex resizes the terminal to a narrower width
      Then the top-half sub-splits remain in a 2:1 width ratio at the new size

  Rule: New splits default to equal weight

    Example: Creating a vertical split from a single view produces two equal halves
      Given Alex has a single split open
      When Alex creates a vertical split
      Then both splits have equal widths

    Example: Adding a third vertical split produces three equal sections
      Given Alex has two equal vertical splits
      When Alex creates another vertical split
      Then all three splits have equal widths

    Example: Removing a split preserves the weights of the remaining splits
      Given Alex has three vertical splits with widths in a 3:2:1 ratio
      When Alex closes the middle split
      Then the remaining two splits retain their original widths proportionally

  Rule: Minimum split dimensions are always enforced regardless of weights

    Example: A split assigned a very small weight still meets the minimum width
      Given Alex has two vertical splits in a narrow terminal
      When the left split's weight would make the right split narrower than 10 columns
      Then the right split is rendered at a minimum of 10 columns wide

    Example: A split assigned a very small weight still meets the minimum height
      Given Alex has two horizontal splits in a very short terminal
      When the top split's weight would make the bottom split shorter than 3 rows
      Then the bottom split is rendered at a minimum of 3 rows tall

  Rule: Split sizes cover the full available area without gaps or overlaps

    Example: Three vertical splits fill the container width exactly
      Given Alex has three vertical splits
      When the layout is computed
      Then the sum of all split widths plus the separator gaps equals the full container width exactly

    Example: The last split in a row absorbs any rounding remainder
      Given Alex has three vertical splits whose exact proportional widths don't divide evenly
      When the layout is computed
      Then no pixel gap appears between the last split and the right edge of the container
