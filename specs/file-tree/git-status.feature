@file-tree @git-status
Feature: Git status refresh lifecycle in the file tree

  Alex the developer sees accurate, up-to-date git status for every file in
  the tree without manually triggering a refresh. Status colors distinguish
  between staged additions, unstaged modifications, untracked files, deletions,
  and conflicts.

  Background:
    Given the file tree sidebar is visible
    And the project is a git repository

  Rule: Git status is automatically scheduled when the tree is first opened

    Example: A git refresh is pending immediately after the tree is created
      Given Alex opens the file tree for the first time
      Then a git refresh is pending

    Example: Git refresh fires during the first process_updates call after debounce
      Given Alex opens the file tree for the first time
      When the git refresh deadline has elapsed
      And process_updates runs with diff providers
      Then the git status map is populated

  Rule: Untracked files have a distinct status from staged-added files

    Example: An untracked file reports Untracked, not Added
      Given new_file.rs has never been committed
      When Alex looks at the file tree
      Then the row for new_file.rs has Untracked status

    Example: A staged-new file reports Added, not Untracked
      Given staged.rs has been staged for addition
      When Alex looks at the file tree
      Then the row for staged.rs has Added status

    Example: Untracked and Added files in the same directory have different statuses
      Given new_file.rs has never been committed
      And staged.rs has been staged for addition
      When Alex looks at the file tree
      Then the row for new_file.rs has Untracked status
      And the row for staged.rs has Added status

    Example: A staged modification to a tracked file reports Staged
      Given src/main.rs has been staged for commit
      When Alex looks at the file tree
      Then the row for src/main.rs has Staged status

  Rule: Git status is refreshed after filesystem operations

    Example: Creating a file via the tree schedules a git refresh
      Given the file tree shows the project root
      When a new file is created via the tree
      Then a git refresh is pending

    Example: An externally detected change schedules a git refresh
      Given the file tree shows the project root
      When an external file change is detected in the project
      Then a git refresh is pending

  Rule: Directory status reflects the worst status of its descendants

    Example: Directory with a conflict child shows Conflict
      Given src/lib.rs has unresolved merge conflicts
      And src/main.rs has uncommitted edits
      When Alex looks at the file tree
      Then src/ has Conflict status

    Example: Directory with only untracked children shows Untracked
      Given new_file.rs has never been committed
      When Alex looks at the file tree
      Then the row for new_file.rs has Untracked status

    Example: A fully clean directory shows Clean
      Given all files in tests/ are committed and unchanged
      When Alex looks at the row for tests/
      Then tests/ uses the default directory color

  Rule: Git status map is cleared before each new refresh cycle

    Example: Stale entries are removed when a new refresh starts
      Given src/main.rs was previously Modified
      When a new git refresh cycle starts
      And src/main.rs is not reported by git this cycle
      Then src/main.rs has Clean status after the refresh
