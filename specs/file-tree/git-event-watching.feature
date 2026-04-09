@file-tree @git-status @file-watching
Feature: Git status refresh driven by git repository events

  Alex the developer sees the file tree reflect git repository state changes —
  commits, checkouts, rebases, and stash operations — automatically, without
  needing to save a file or press R.

  Background:
    Given the file tree sidebar is visible
    And the project is a git repository

  Rule: Switching branches refreshes git status for all files

    Example: Checking out a branch updates modified-file indicators
      Given src/main.rs shows Modified status on the current branch
      When Alex runs "git checkout feature-branch" in the terminal
      And the file tree processes pending git events
      Then the git status for all files is refreshed
      And src/main.rs status reflects its state on feature-branch

    Example: Checking out a branch that has untracked files in the index shows them
      Given new_feature.rs does not exist on the current branch
      When Alex checks out a branch where new_feature.rs is untracked
      And the file tree processes pending git events
      Then new_feature.rs appears with Untracked status

  Rule: Committing staged files clears their Modified or Staged status

    Example: A commit removes Staged status from committed files
      Given src/main.rs shows Staged status
      When Alex runs "git commit -m 'fix: update main'" in the terminal
      And the file tree processes pending git events
      Then src/main.rs shows Clean status

  Rule: Rebasing updates status for all files touched by the rebase

    Example: Starting a rebase refreshes the status of affected files
      Given src/main.rs shows Clean status
      When Alex starts an interactive rebase that edits src/main.rs
      And the file tree processes pending git events
      Then the git status for src/main.rs is refreshed

  Rule: Stash push and pop refresh affected file statuses

    Example: Stashing changes marks previously modified files as Clean
      Given src/main.rs shows Modified status
      When Alex runs "git stash" in the terminal
      And the file tree processes pending git events
      Then src/main.rs shows Clean status

    Example: Popping a stash marks previously clean files as Modified
      Given src/main.rs shows Clean status
      When Alex runs "git stash pop" in the terminal
      And the file tree processes pending git events
      Then src/main.rs shows Modified status

  Rule: Merge conflicts are detected without a file save

    Example: A merge that creates conflicts shows Conflict status automatically
      Given src/main.rs shows Clean status
      When Alex runs "git merge conflicting-branch" in the terminal
      And the merge results in a conflict in src/main.rs
      And the file tree processes pending git events
      Then src/main.rs shows Conflict status
      And the parent directory shows Conflict status

  Rule: Lock files created by git operations do not trigger spurious refreshes

    Example: A .git/index.lock file does not cause a premature git refresh
      Given no git operation is in progress
      When a .git/index.lock file is created by an external git client
      Then no git refresh is triggered until the lock file is removed
