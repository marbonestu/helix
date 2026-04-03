@file-tree @split-picker
Feature: Open file in a chosen split from the file tree

  Alex the developer can open files from the file tree into a specific editor
  split. When only one split is open the file opens immediately; when multiple
  splits are open a labelled overlay appears so Alex can choose the target split
  by pressing a letter key.

  Background:
    Given Alex has opened Helix in a project directory

  Rule: With a single split the file opens without a picker

    Example: Enter opens the file directly when one split exists
      Given the file tree sidebar is visible and focused
      And only one editor split is open
      And the file tree selection is on src/main.rs
      When Alex presses enter
      Then src/main.rs is open in the editor
      And no split-picker overlay is shown

  Rule: With multiple splits a labelled picker is shown

    Example: Enter shows split labels when two splits are open
      Given the file tree sidebar is visible and focused
      And two editor splits are open
      And the file tree selection is on src/main.rs
      When Alex presses enter
      Then the split-picker overlay is shown
      And each split displays a unique letter label

    Example: Pressing a label opens the file in the matching split
      Given the split-picker overlay is showing for src/main.rs
      When Alex presses the label for the second split
      Then src/main.rs is open in the second split
      And the split-picker overlay is dismissed

    Example: Pressing Esc cancels without opening the file
      Given the split-picker overlay is showing for src/main.rs
      When Alex presses escape on the split picker
      Then the split-picker overlay is dismissed
      And no file is opened
