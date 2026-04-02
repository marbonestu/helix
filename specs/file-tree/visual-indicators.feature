@file-tree @visual
Feature: Visual indicators in the file tree

  Alex the developer can see git status through colored filenames, file type
  icons, and theme-aware colors in the file tree, giving a quick overview of
  project state without opening each file.

  Background:
    Given the file tree sidebar is visible
    And the project is a git repository

  Rule: Git-changed files are highlighted with status colors on the filename

    Example: A modified tracked file has its name shown in the modified color
      Given src/main.rs has uncommitted edits
      When Alex looks at the file tree
      Then the row for main.rs is styled with the modified color

    Example: An untracked file has its name shown in the untracked color
      Given new_file.rs has never been committed
      When Alex looks at the file tree
      Then the row for new_file.rs is styled with the untracked color

    Example: A file with merge conflicts is highlighted with the conflict color
      Given src/lib.rs has unresolved merge conflicts
      When Alex looks at the file tree
      Then the row for lib.rs is styled with the conflict color

    Example: A deleted staged file is highlighted with the deleted color
      Given old.rs has been staged for deletion
      When Alex looks at the file tree
      Then the row for old.rs is styled with the deleted color

    Example: A clean file uses the normal file color
      Given README.md has no uncommitted changes
      When Alex looks at the file tree
      Then the row for README.md uses the default file color

  Rule: A directory's status color reflects the worst status among its descendants

    Example: Directory is colored modified when any descendant is modified
      Given src/main.rs has uncommitted edits
      And all other files in src/ are clean
      When Alex looks at the row for src/
      Then src/ is styled with the modified color

    Example: Directory is colored conflict when a descendant has conflicts
      Given src/lib.rs has merge conflicts
      And src/main.rs is only modified
      When Alex looks at the row for src/
      Then src/ is styled with the conflict color

    Example: Clean directory uses the default directory color
      Given all files in tests/ are committed and unchanged
      When Alex looks at the row for tests/
      Then tests/ uses the default directory color

  Rule: File type icons are shown when icons are enabled in config

    Example: A Rust source file shows the Rust icon
      Given the config has file_tree.icons = true
      And the project contains main.rs
      When Alex looks at the file tree
      Then the row for main.rs shows the Rust language icon

    Example: A directory shows a folder icon based on expanded state
      Given the config has file_tree.icons = true
      When Alex looks at a collapsed directory
      Then it shows a closed folder icon
      When Alex expands the directory
      Then it shows an open folder icon

    Example: Icons are hidden when disabled in config
      Given the config has file_tree.icons = false
      When Alex looks at the file tree
      Then no icon characters appear before any filenames

  Rule: The sidebar respects the active theme's color scopes

    Example: Sidebar background uses ui.sidebar scope
      Given the active theme defines a color for ui.sidebar
      When the file tree sidebar is visible
      Then the sidebar background uses that color

    Example: Selected row uses ui.sidebar.selected when focused
      Given the active theme defines ui.sidebar.selected
      And keyboard focus is in the file tree
      When Alex selects a row
      Then that row is highlighted using the ui.sidebar.selected color

    Example: Directory names use ui.sidebar.directory scope
      Given the active theme defines ui.sidebar.directory
      When Alex looks at a directory row
      Then the directory name is rendered in the ui.sidebar.directory color

    Example: Sidebar falls back to ui.statusline when ui.sidebar is not defined
      Given the active theme does not define ui.sidebar
      When the file tree sidebar is visible
      Then the sidebar background uses the ui.statusline color as a fallback

  Rule: The follow-current-file feature keeps the tree in sync with the active buffer

    Example: Switching to a buffer reveals its file in the tree
      Given the file tree sidebar is visible
      And Alex switches to a buffer for src/lib.rs
      When enough time passes for the follow debounce
      Then src/ is expanded in the tree
      And lib.rs is selected and scrolled into view

    Example: Follow does not interrupt manual tree navigation
      Given keyboard focus is in the file tree sidebar
      And Alex is navigating with j/k
      When Alex switches to a different buffer in another window
      Then the tree selection does not jump away from Alex's current position

    Example: Follow triggers again after focus returns to the editor
      Given Alex was navigating the tree manually
      And Alex returns focus to the editor
      When Alex switches to a buffer for tests/integration.rs
      Then integration.rs becomes selected in the tree
