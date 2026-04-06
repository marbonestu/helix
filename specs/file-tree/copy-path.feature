@file-tree @clipboard @copy-path
Feature: Copying file paths to the system clipboard from the file tree

  Alex the developer can press Y on any file or directory in the file tree
  to copy its absolute path directly to the system clipboard, ready to paste
  into a terminal, another application, or anywhere outside Helix.

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
        .gitignore
        Cargo.toml
      """

  Rule: Y copies the absolute path of the selected item to the system clipboard

    Example: Y on a file copies its full absolute path
      Given main.rs is selected
      When Alex presses Y
      Then the system clipboard contains the absolute path to "src/main.rs"
      And a status message confirms "Copied path: <absolute-path-to-src/main.rs>"

    Example: Y on a directory copies the directory's full absolute path
      Given src/ is selected
      When Alex presses Y
      Then the system clipboard contains the absolute path to "src/"
      And a status message confirms "Copied path: <absolute-path-to-src>"

    Example: Y on a root-level file copies its full absolute path
      Given Cargo.toml is selected
      When Alex presses Y
      Then the system clipboard contains the absolute path to "Cargo.toml"

  Rule: Y does not affect the internal file-tree clipboard

    Example: Pressing Y does not change the internal yank/cut clipboard
      Given main.rs is in the internal clipboard with operation "copy"
      When Alex selects lib.rs and presses Y
      Then the system clipboard contains the absolute path to "src/lib.rs"
      And the internal clipboard still holds main.rs with operation "copy"

  Rule: Y overwrites the previous system clipboard contents

    Example: A second Y on a different file replaces the previous path
      Given Alex previously pressed Y on main.rs
      When Alex selects lib.rs and presses Y
      Then the system clipboard contains the absolute path to "src/lib.rs"
      And the path to main.rs is no longer in the system clipboard

  Rule: Feedback is shown after copying a path

    Example: A status line message confirms the copied path
      Given main.rs is selected
      When Alex presses Y
      Then the status line shows a message starting with "Copied path:"
      And the message includes the full absolute path

    Example: Y on a directory shows the directory path in the status message
      Given src/ is selected
      When Alex presses Y
      Then the status line shows a message starting with "Copied path:"
      And the message includes the full absolute path to src/
