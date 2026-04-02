@splits:resize @splits:zoom
Feature: Resize and zoom interaction

  When Alex the developer resizes splits and then uses zoom, the zoom
  should temporarily override the proportions but unzooming must restore
  the layout exactly as it was before the zoom — not before the resize.

  Rule: Unzoom restores the most recent pre-zoom layout, not the original equal layout

    Example: Resize then zoom then unzoom preserves the resized proportions
      Given Alex has two vertical splits with equal widths
      And Alex has resized the left split to be twice as wide as the right
      When Alex zooms the left split and then unzooms
      Then the left split is twice as wide as the right split

    Example: Multiple resizes before zoom are all preserved after unzoom
      Given Alex has three vertical splits with equal widths
      And Alex has resized the splits to a 3:2:1 width ratio
      When Alex zooms the first split and then unzooms
      Then the splits return to the 3:2:1 width ratio

  Rule: Resizing while zoomed modifies the zoomed weights but those changes are discarded on unzoom

    Example: Resize during zoom is not preserved after unzoom
      Given Alex has two vertical splits resized to a 2:1 ratio
      And Alex has zoomed the left split
      When Alex resizes the left split further and then unzooms
      Then the splits return to the 2:1 ratio from before the zoom
      And the resize performed during zoom is discarded
