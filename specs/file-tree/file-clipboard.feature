@file-tree @file-management @clipboard
Feature: Copying, cutting, pasting, and duplicating files in the file tree

  Alex the developer can copy or cut files and directories in the file tree
  and paste them elsewhere in the project, using an internal clipboard that
  mirrors the Vim yank/delete/put mental model.

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
        archive/
        Cargo.toml
        README.md
      """

  Rule: y yanks the selected item into the internal clipboard

    Example: y on a file marks it as copied in the clipboard
      Given main.rs is selected
      When Alex presses y
      Then main.rs is recorded in the clipboard with operation "copy"
      And a status message confirms "Yanked: src/main.rs"
      And main.rs remains on disk unchanged

    Example: y on a directory marks the whole directory as copied
      Given src/ is selected
      When Alex presses y
      Then src/ is recorded in the clipboard with operation "copy"
      And src/ and its contents remain on disk unchanged

    Example: Yanking a second item replaces the previous clipboard entry
      Given main.rs was previously yanked
      When Alex selects lib.rs and presses y
      Then the clipboard now holds lib.rs
      And main.rs is no longer in the clipboard

  Rule: x cuts the selected item into the internal clipboard

    Example: x on a file marks it as cut in the clipboard
      Given main.rs is selected
      When Alex presses x
      Then main.rs is recorded in the clipboard with operation "cut"
      And a status message confirms "Cut: src/main.rs"
      And main.rs remains on disk until pasted

    Example: x on a directory marks the whole directory as cut
      Given src/ is selected
      When Alex presses x
      Then src/ is recorded in the clipboard with operation "cut"
      And src/ remains on disk until pasted

  Rule: p pastes the clipboard item into the target location

    Example: p with a directory selected pastes inside that directory
      Given lib.rs is in the clipboard with operation "copy"
      And archive/ is selected
      When Alex presses p
      Then archive/lib.rs is created as a copy of src/lib.rs
      And the file tree refreshes to show archive/lib.rs
      And the selection moves to the pasted file

    Example: p with a file selected pastes into the file's parent directory
      Given lib.rs is in the clipboard with operation "copy"
      And Cargo.toml is selected
      When Alex presses p
      Then lib.rs is copied into the project root alongside Cargo.toml
      And the file tree refreshes to show the pasted file

    Example: p after a cut moves the file to the new location
      Given main.rs is in the clipboard with operation "cut"
      And archive/ is selected
      When Alex presses p
      Then src/main.rs is moved to archive/main.rs on disk
      And the file tree no longer shows main.rs under src/
      And archive/main.rs appears in the tree

    Example: Cut clipboard is cleared after a successful paste
      Given main.rs is in the clipboard with operation "cut"
      And archive/ is selected
      When Alex presses p
      Then the clipboard is empty
      And pressing p again has no effect

    Example: Yank clipboard persists after a paste
      Given lib.rs is in the clipboard with operation "copy"
      And archive/ is selected
      When Alex presses p
      Then archive/lib.rs is created
      And lib.rs remains in the clipboard with operation "copy"
      And Alex can press p again to paste another copy

  Rule: Pasting over an existing filename shows an error

    Example: Name collision on paste is rejected with an error
      Given lib.rs is in the clipboard with operation "copy"
      And src/ is selected
      When Alex presses p
      Then the paste does not overwrite the existing src/lib.rs
      And an error message reads "File already exists: src/lib.rs"

  Rule: D duplicates the selected file in place with a new name prompt

    Example: D on a file opens a prompt pre-filled with a suggested copy name
      Given main.rs is selected
      When Alex presses D
      Then a duplication prompt appears at the bottom of the sidebar
      And the prompt input is pre-filled with "main.copy.rs"

    Example: Confirming the prompt creates the duplicate alongside the original
      Given the duplication prompt is active pre-filled with "main.copy.rs"
      When Alex presses Enter
      Then src/main.copy.rs is created as a copy of src/main.rs
      And the file tree shows both main.rs and main.copy.rs
      And the selection moves to the duplicate file

    Example: Alex can change the suggested name before confirming
      Given the duplication prompt is active pre-filled with "main.copy.rs"
      When Alex clears the input, types "main_backup.rs", and presses Enter
      Then src/main_backup.rs is created as a copy of src/main.rs

    Example: Escape cancels the duplication without creating any file
      Given the duplication prompt is active pre-filled with "main.copy.rs"
      When Alex presses Escape
      Then the duplication prompt disappears
      And no new file is created on disk

  Rule: D is not available for directories

    Example: D on a directory shows an info message
      Given src/ is selected
      When Alex presses D
      Then no duplication prompt appears
      And a message reads "Duplicate is only available for files"

  Rule: The clipboard item is marked with a dimmed tag in the tree

    Example: A yanked file shows a dimmed (C) tag after its name
      Given lib.rs is in the clipboard with operation "copy"
      When Alex looks at the file tree
      Then the row for lib.rs shows a dimmed "(C)" tag after the filename
      And the rest of the row styling is unchanged

    Example: A cut file shows a dimmed (X) tag after its name
      Given main.rs is in the clipboard with operation "cut"
      When Alex looks at the file tree
      Then the row for main.rs shows a dimmed "(X)" tag after the filename

    Example: The tag is removed after a successful paste
      Given main.rs is in the clipboard with operation "cut"
      And Alex pastes main.rs into archive/
      When Alex looks at the file tree
      Then no row shows an "(X)" tag

    Example: The (C) tag persists across navigation
      Given lib.rs is in the clipboard with operation "copy"
      When Alex navigates the tree with j and k
      Then the row for lib.rs still shows the dimmed "(C)" tag

    Example: Only one item shows a tag at a time
      Given lib.rs was previously yanked and shows "(C)"
      When Alex yanks main.rs
      Then main.rs shows a dimmed "(C)" tag
      And lib.rs no longer shows any clipboard tag

  Rule: p is a no-op when the clipboard is empty

    Example: p with nothing in the clipboard does nothing
      Given the clipboard is empty
      When Alex presses p
      Then no files are created or moved
      And the tree is unchanged
