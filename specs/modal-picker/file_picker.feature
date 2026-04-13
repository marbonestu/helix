@modal-picker:file-picker
Feature: File picker built on ModalPicker

  The file picker is an instance of the ModalPicker component. It lets Alex
  the developer fuzzy-find files in the workspace and open one or many at
  once. It demonstrates that multi-select and modal modes are provided by
  the ModalPicker component, not re-implemented per picker.

  Background:
    Given Alex is in a workspace with multiple files
    And Alex opens the file picker

  Rule: Enter in Normal mode opens the file under the cursor in the current window

    Example: Enter opens the selected file and closes the picker
      Given the picker is in Normal mode
      And the cursor is on "src/main.rs"
      When Alex presses "<Enter>"
      Then "src/main.rs" is opened in the current window
      And the picker is closed

  Rule: With a selection, Enter opens all selected files as background buffers and focuses the first

    Example: Enter with multiple selected files loads them all and focuses the first
      Given "src/foo.rs", "src/bar.rs", and "src/baz.rs" are marked as selected
      When Alex presses "<Enter>"
      Then "src/foo.rs", "src/bar.rs", and "src/baz.rs" are all opened as buffers
      And the editor focuses "src/foo.rs"
      And the picker is closed

    Example: Enter with no selection opens only the cursor file in the current window
      Given no files are selected
      And the cursor is on "src/main.rs"
      When Alex presses "<Enter>"
      Then only "src/main.rs" is opened in the current window

  Rule: Ctrl+S opens the file under the cursor in a horizontal split regardless of selection

    Example: Ctrl+S always opens a single file in a horizontal split
      Given "src/foo.rs" and "src/bar.rs" are marked as selected
      And the cursor is on "src/baz.rs"
      When Alex presses "<C-s>"
      Then only "src/baz.rs" is opened in a horizontal split
      And the picker is closed

  Rule: The file picker filters by filename using the same fuzzy engine as other pickers

    Example: Typing a partial path narrows the list to matching files
      Given the picker is in Insert mode
      When Alex types "term/ui"
      Then the list shows only files whose paths contain "term/ui"
