@file-tree @file-management @rename
Feature: Renaming files and directories from the file tree

  Alex the developer can rename any file or directory directly from the file
  tree using an inline prompt pre-filled with the current name, so only the
  parts that need changing require keystrokes.

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

  Rule: r opens an inline rename prompt pre-filled with the current name

    Example: r on a file pre-fills the prompt with the filename
      Given main.rs is selected
      When Alex presses r
      Then a rename prompt appears at the bottom of the sidebar
      And the prompt input is pre-filled with "main.rs"

    Example: r on a directory pre-fills the prompt with the directory name
      Given src/ is selected
      When Alex presses r
      Then a rename prompt appears at the bottom of the sidebar
      And the prompt input is pre-filled with "src"

    Example: R refreshes the tree from disk (r is rename, R is refresh)
      Given the file tree is showing an outdated listing
      When Alex presses R
      Then the tree re-scans the filesystem
      And any added or removed files are reflected in the listing

  Rule: Confirming the prompt renames the item on disk

    Example: Confirming a modified name renames the file
      Given the rename prompt is active pre-filled with "main.rs"
      And Alex clears the input and types "app.rs"
      When Alex presses Enter
      Then src/main.rs is renamed to src/app.rs on disk
      And the file tree refreshes to show app.rs in place of main.rs
      And the selection stays on the renamed file

    Example: Renaming a directory renames it on disk along with its contents
      Given the rename prompt is active pre-filled with "src"
      And Alex clears the input and types "source"
      When Alex presses Enter
      Then the src/ directory is renamed to source/ on disk
      And all files within it remain accessible under the new path

    Example: Confirming without any change leaves the item untouched
      Given the rename prompt is active pre-filled with "main.rs"
      When Alex presses Enter without changing the text
      Then no rename occurs on disk
      And the file tree is unchanged

  Rule: Escape cancels the rename without modifying the filesystem

    Example: Escape dismisses the prompt and restores selection
      Given the rename prompt is active pre-filled with "main.rs"
      And Alex has edited the text to "app.rs"
      When Alex presses Escape
      Then the rename prompt disappears
      And main.rs is unchanged on disk
      And the selection returns to main.rs

  Rule: Renaming to a name that already exists shows an error

    Example: Duplicate name is rejected with an error message
      Given the rename prompt is active pre-filled with "main.rs"
      And Alex clears the input and types "lib.rs"
      When Alex presses Enter
      Then the rename prompt remains visible
      And an error message reads "File already exists: src/lib.rs"
      And main.rs is unchanged on disk

  Rule: Open buffers for renamed files are updated to the new path

    Example: A buffer for the renamed file tracks the new path
      Given main.rs is open in the editor
      And the rename prompt is active pre-filled with "main.rs"
      When Alex renames it to "app.rs" and confirms
      Then the editor buffer path updates to src/app.rs
      And the buffer title in the status line reflects the new name

  Rule: Renaming notifies the language server so references in other files are updated

    Example: LSP willRename workspace edit is applied before the file moves
      Given a language server is active that supports willRenameFiles
      And main.rs is imported by src/lib.rs as "mod main"
      When Alex renames main.rs to app.rs and confirms
      Then the language server receives a willRenameFiles request
      And any workspace edits it returns are applied to open buffers
      And src/lib.rs is updated to reference "mod app"

    Example: LSP didRename notification is sent after the file is moved
      Given a language server is active that supports didRenameFiles
      When Alex renames main.rs to app.rs and confirms
      Then after the file is moved the language server receives a didRenameFiles notification

    Example: Rename proceeds normally when no language server is active
      Given no language server is running for the current project
      When Alex renames main.rs to app.rs and confirms
      Then src/main.rs is renamed to src/app.rs on disk
      And no LSP errors are shown
