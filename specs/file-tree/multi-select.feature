@file-tree @file-ops
Feature: Multi-select files for batch operations

  Alex the developer can mark multiple files and directories for batch deletion,
  copy, or move without performing each operation individually.

  Background:
    Given the file tree sidebar is visible and focused
    And the project contains the structure:
      """
      project/
        src/
          main.rs
          lib.rs
          utils.rs
        tests/
          integration.rs
          unit.rs
        Cargo.toml
        README.md
      """

  Rule: v toggles selection on the focused row without moving focus

    Example: v marks an unselected file
      Given main.rs is focused and not selected
      When Alex presses v
      Then main.rs is marked as selected
      And the focused row remains on main.rs

    Example: v on a selected file removes its selection mark
      Given main.rs is focused and marked as selected
      When Alex presses v
      Then main.rs is no longer marked as selected

    Example: Multiple files can be marked independently
      Given main.rs is focused
      When Alex presses v
      And Alex moves focus to lib.rs and presses v
      Then main.rs and lib.rs are both marked as selected

  Rule: Directories can be selected alongside files

    Example: v on a directory marks it for batch operations
      Given src/ is focused and not selected
      When Alex presses v
      Then src/ is marked as selected

  Rule: d deletes all selected entries after confirmation

    Example: Pressing d with two files selected prompts to delete both
      Given main.rs and lib.rs are marked as selected
      When Alex presses d
      Then a confirmation prompt names both main.rs and lib.rs
      And pressing y deletes both files from the filesystem
      And neither file appears in the tree afterward

    Example: Cancelling a batch delete leaves all files intact
      Given main.rs and lib.rs are marked as selected
      When Alex presses d
      And Alex presses Escape at the confirmation prompt
      Then both main.rs and lib.rs remain in the tree

  Rule: y copies and x cuts all selected entries to the clipboard

    Example: Pressing y with three files selected stages all three for copy
      Given main.rs, lib.rs, and utils.rs are marked as selected
      When Alex presses y
      Then the clipboard holds a copy operation for all three files
      And the selection marks are cleared

    Example: Pressing x with two files stages them for move
      Given main.rs and lib.rs are marked as selected
      When Alex presses x
      Then the clipboard holds a cut operation for both files

  Rule: p pastes all clipboard entries into the focused directory

    Example: Pasting a copied selection duplicates files into the target directory
      Given main.rs and lib.rs are in the clipboard as a copy operation
      And tests/ is focused and expanded
      When Alex presses p
      Then copies of main.rs and lib.rs appear inside tests/
      And the originals remain in src/

    Example: Pasting a cut selection moves files into the target directory
      Given main.rs and lib.rs are in the clipboard as a cut operation
      And tests/ is focused
      When Alex presses p
      Then main.rs and lib.rs appear inside tests/
      And they no longer exist in src/

  Rule: Escape clears all selection marks without performing any operation

    Example: Escape exits multi-select mode and removes all marks
      Given main.rs and lib.rs are marked as selected
      When Alex presses Escape
      Then no files are marked as selected

  Rule: Single-file operations work as before when nothing is selected

    Example: d on a focused file with no selection marks deletes only that file
      Given no files are marked as selected
      And main.rs is focused
      When Alex presses d and confirms
      Then only main.rs is deleted
      And lib.rs and utils.rs remain in the tree
