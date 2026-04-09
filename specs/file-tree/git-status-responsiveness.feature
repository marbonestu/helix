@file-tree @git-status @performance
Feature: Responsive git status refresh with two-phase scanning

  Alex the developer sees git status update immediately after the first triggering
  event, with rapid successive events coalesced into a single follow-up scan.
  Tracked-file statuses appear instantly while untracked files stream in shortly after.

  Background:
    Given the file tree sidebar is visible
    And the project is a git repository

  Rule: The first git refresh request fires immediately, not after a delay

    Example: Saving a file triggers an instant git status scan
      Given no git refresh is in progress
      When Alex saves src/main.rs
      Then a git refresh starts immediately
      And the tree shows updated status for src/main.rs without a 1-second wait

    Example: Opening the file tree triggers an immediate git refresh
      Given the file tree has just been opened
      When the first process_updates call runs
      Then a git refresh starts immediately

    Example: A second save while a refresh is in progress is deferred, not dropped
      Given a git refresh is currently in progress
      When Alex saves src/lib.rs
      Then a deferred git refresh is scheduled for after the current one completes

  Rule: Rapid successive events produce at most one additional refresh

    Example: Five rapid saves result in two git scans, not six
      Given no git refresh is in progress
      When Alex saves 5 different files within 200 milliseconds
      Then exactly one git refresh runs immediately
      And exactly one more git refresh runs after the burst settles

    Example: Changes during a refresh are not silently discarded
      Given a git refresh started when src/main.rs was saved
      When src/lib.rs is also saved before the refresh completes
      Then a second git refresh runs after the first one finishes
      And the status for src/lib.rs is updated

  Rule: Tracked-file statuses appear before untracked files are scanned

    Example: Modified tracked files are visible before the full scan finishes
      Given src/main.rs is a tracked file with uncommitted edits
      And new_feature.rs is an untracked file in the same directory
      When a git refresh starts
      Then src/main.rs shows Modified status before new_feature.rs status is resolved

    Example: The second phase adds untracked status without clearing tracked results
      Given the first scan phase has completed and shows src/main.rs as Modified
      When the second scan phase completes
      Then new_feature.rs shows Untracked status
      And src/main.rs still shows Modified status

    Example: A repository with no untracked files completes after the first phase
      Given all files in the repository are tracked
      When a git refresh runs
      Then the refresh completes after a single scan phase
