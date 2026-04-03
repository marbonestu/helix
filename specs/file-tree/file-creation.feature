@file-tree @file-management @creation
Feature: Creating files and directories from the file tree

  Alex the developer can create new files and directories directly from the file
  tree sidebar without leaving the keyboard, using inline prompts at the bottom
  of the panel.

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

  Rule: a opens an inline prompt to create a new file

    Example: a on a directory prompts for a filename inside that directory
      Given src/ is selected
      When Alex presses a
      Then a creation prompt appears at the bottom of the sidebar
      And the prompt reads "New file: src/"

    Example: a on a file prompts for a filename inside its parent directory
      Given main.rs is selected
      When Alex presses a
      Then a creation prompt appears at the bottom of the sidebar
      And the prompt reads "New file: src/"

    Example: Confirming the prompt creates the file on disk
      Given the creation prompt is active with path "src/"
      And Alex has typed "config.rs"
      When Alex presses Enter
      Then src/config.rs is created on disk
      And the file tree refreshes to show src/config.rs
      And the selection moves to the newly created file

    Example: The new file is opened in the editor after creation
      Given the creation prompt is active with path "src/"
      And Alex has typed "config.rs"
      When Alex presses Enter
      Then src/config.rs opens in the editor view

    Example: Escape cancels the prompt without creating any file
      Given the creation prompt is active with path "src/"
      And Alex has typed "config.rs"
      When Alex presses Escape
      Then the creation prompt disappears
      And no new file is created on disk
      And the previous selection is restored

  Rule: Nested path separators in the filename create intermediate directories

    Example: Typing a path with a slash creates the intermediate directory
      Given src/ is selected
      And the creation prompt is active with path "src/"
      When Alex types "models/user.rs" and presses Enter
      Then src/models/ is created as a directory
      And src/models/user.rs is created as a file
      And the tree expands to show the new structure

  Rule: Creating a file that already exists shows an error

    Example: Duplicate filename is rejected with an error message
      Given the creation prompt is active with path "src/"
      And Alex types "main.rs"
      When Alex presses Enter
      Then the creation prompt remains visible
      And an error message reads "File already exists: src/main.rs"
      And no file is overwritten

  Rule: A prompts for a new directory name

    Example: A on a directory prompts for a subdirectory name
      Given src/ is selected
      When Alex presses A
      Then a creation prompt appears at the bottom of the sidebar
      And the prompt reads "New directory: src/"

    Example: A on a file prompts inside the file's parent directory
      Given main.rs is selected
      When Alex presses A
      Then a creation prompt appears at the bottom of the sidebar
      And the prompt reads "New directory: src/"

    Example: Confirming the prompt creates the directory on disk
      Given the directory creation prompt is active with path "src/"
      And Alex has typed "models"
      When Alex presses Enter
      Then src/models/ is created as a directory on disk
      And the file tree refreshes to show src/models/
      And the selection moves to the newly created directory

    Example: Creating a directory that already exists shows an error
      Given the directory creation prompt is active with path "src/"
      And Alex types "tests"
      When Alex presses Enter
      Then the creation prompt remains visible
      And an error message reads "Directory already exists: src/tests"

  Rule: Creation prompt accepts Backspace to edit the typed name

    Example: Backspace removes the last typed character
      Given the creation prompt is active and Alex has typed "confg"
      When Alex presses Backspace
      Then the prompt text becomes "conf"

    Example: Backspace on an empty input keeps the prompt active
      Given the creation prompt is active with no text typed
      When Alex presses Backspace
      Then the prompt remains active with empty input
