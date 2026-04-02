@file-tree @visual
Feature: Visual indicators in the file tree

  Alex the developer can see git status, file type icons, and theme-aware colors
  in the file tree, giving a quick overview of project state without opening each file.

  Background:
    Given the file tree sidebar is visible
    And the project is a git repository

  Rule: Git status symbols appear next to modified files

    Example: A modified tracked file shows a filled dot
      Given src/main.rs has uncommitted edits
      When Alex looks at the file tree
      Then the row for main.rs displays a filled dot symbol (●)

    Example: An untracked file shows a hollow dot
      Given new_file.rs has never been committed
      When Alex looks at the file tree
      Then the row for new_file.rs displays a hollow dot symbol (◌)

    Example: A file with merge conflicts shows a warning symbol
      Given src/lib.rs has unresolved merge conflicts
      When Alex looks at the file tree
      Then the row for lib.rs displays a warning symbol (⚠)

    Example: A deleted (staged) file shows a cross symbol
      Given old.rs has been staged for deletion
      When Alex looks at the file tree
      Then the row for old.rs displays a cross symbol (✕)

    Example: A clean file shows no status symbol
      Given README.md has no uncommitted changes
      When Alex looks at the file tree
      Then the row for README.md shows no git status symbol

  Rule: A directory's status reflects the worst status among its descendants

    Example: Directory shows modified when any descendant is modified
      Given src/main.rs has uncommitted edits
      And all other files in src/ are clean
      When Alex looks at the row for src/
      Then src/ displays the modified symbol (●)

    Example: Directory shows conflict when a descendant has conflicts
      Given src/lib.rs has merge conflicts
      And src/main.rs is only modified
      When Alex looks at the row for src/
      Then src/ displays the conflict symbol (⚠)

    Example: Clean directory shows no symbol
      Given all files in tests/ are committed and unchanged
      When Alex looks at the row for tests/
      Then tests/ shows no git status symbol

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
