@file-tree @navigation
Feature: Bulk expand and collapse of directory subtrees

  Alex the developer can expand an entire directory subtree or collapse all open
  directories to the root level with a single key press, avoiding tedious
  one-by-one expansion when exploring unfamiliar codebases.

  Background:
    Given the file tree sidebar is visible and focused
    And the project contains the structure:
      """
      project/
        src/
          models/
            user.rs
            post.rs
          controllers/
            auth.rs
          main.rs
        tests/
          unit/
            user_test.rs
          integration.rs
        Cargo.toml
      """

  Rule: E expands the focused directory and all of its descendants

    Example: E on a collapsed directory recursively expands its subtree
      Given src/ is focused and collapsed
      When Alex presses E
      Then src/ is expanded
      And models/ and controllers/ are expanded beneath it
      And user.rs, post.rs, and auth.rs are all visible

    Example: E on an already-expanded directory expands any collapsed descendants
      Given src/ is focused and expanded
      And models/ is collapsed
      When Alex presses E
      Then models/ becomes expanded
      And user.rs and post.rs are visible

    Example: E on a file does nothing
      Given main.rs is focused
      When Alex presses E
      Then the tree is unchanged

  Rule: E at the root expands the entire tree

    Example: E on the root row reveals every file in the project
      Given the root row is focused
      And all directories are collapsed
      When Alex presses E
      Then src/, models/, controllers/, tests/, and unit/ are all expanded
      And every file in the project is visible

  Rule: C collapses all open directories to the root level

    Example: C collapses all expanded directories regardless of depth
      Given src/, models/, and tests/ are all expanded
      When Alex presses C
      Then src/ is collapsed
      And tests/ is collapsed
      And no subdirectory rows are visible beneath the root

    Example: C does not change which row is selected
      Given models/ is selected and several directories are expanded
      When Alex presses C
      Then all directories are collapsed
      And the selection stays on models/ (now a hidden row scrolled to if needed)

    Example: C on an already fully-collapsed tree does nothing
      Given all directories are already collapsed
      When Alex presses C
      Then the tree is unchanged

  Rule: Expand-all is interrupted if too many nodes would be loaded

    Example: E on a very large directory warns and stops at a depth limit
      Given src/ contains more than 500 descendant files
      When Alex presses E on src/
      Then expansion stops at the configured depth limit
      And Alex sees a status-bar message indicating the limit was reached
