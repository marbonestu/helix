@file-tree @file-management @deletion
Feature: Deleting files and directories from the file tree

  Alex the developer can delete files and directories from the file tree with a
  single key, protected by an explicit confirmation prompt so accidental
  keypresses cannot cause unrecoverable data loss.

  Background:
    Given the file tree sidebar is visible and focused
    And the project contains the structure:
      """
      project/
        src/
          main.rs
          lib.rs
        tests/
          integration.rs
        Cargo.toml
        README.md
      """

  Rule: d shows a confirmation prompt before deleting

    Example: d on a file prompts for confirmation
      Given main.rs is selected
      When Alex presses d
      Then a confirmation prompt appears at the bottom of the sidebar
      And the prompt reads "Delete src/main.rs? (y/n)"

    Example: d on a directory prompts with a recursive warning
      Given src/ is selected
      When Alex presses d
      Then a confirmation prompt appears at the bottom of the sidebar
      And the prompt reads "Delete src/ and all contents? (y/n)"

  Rule: Confirming with y deletes the item from disk

    Example: y confirms and deletes a file
      Given the deletion prompt is showing "Delete src/main.rs? (y/n)"
      When Alex presses y
      Then src/main.rs is removed from disk
      And the file tree refreshes and no longer shows main.rs
      And the selection moves to the nearest remaining row

    Example: y confirms and recursively deletes a directory
      Given the deletion prompt is showing "Delete src/ and all contents? (y/n)"
      When Alex presses y
      Then src/ and all its contents are removed from disk
      And the file tree no longer shows src/ or any of its children

  Rule: Any key other than y cancels the deletion

    Example: n cancels the deletion
      Given the deletion prompt is showing "Delete src/main.rs? (y/n)"
      When Alex presses n
      Then the deletion prompt disappears
      And main.rs remains on disk
      And the selection stays on main.rs

    Example: Escape also cancels the deletion
      Given the deletion prompt is showing "Delete src/main.rs? (y/n)"
      When Alex presses Escape
      Then the deletion prompt disappears
      And main.rs remains on disk

  Rule: Deleting a file that is open in the editor closes its buffer

    Example: Open buffer is closed when its file is deleted
      Given main.rs is open in the editor
      When Alex deletes main.rs and confirms with y
      Then the editor buffer for main.rs is closed
      And the editor switches to another open buffer or shows an empty state

  Rule: Deletion of a read-only or protected file shows an error

    Example: Permission denied results in an error message
      Given a file that Alex does not have write access to is selected
      When Alex presses d and confirms with y
      Then the file is not deleted
      And an error message reads "Permission denied"
      And the file tree is unchanged
