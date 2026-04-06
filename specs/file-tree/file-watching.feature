Feature: File tree watches for external filesystem changes
  As a developer working in Helix
  I want the file tree sidebar to reflect external changes automatically
  So that I always see an up-to-date view without manual refreshing

  Background:
    Given the file tree sidebar is visible and focused

  @file-tree @file-watching @happy-path
  Rule: A new file created externally appears in the tree

    Scenario: External file creation is reflected in the tree
      Given the file tree shows the project root
      When a new file "new_file.rs" is created externally in the root directory
      And the file tree processes pending updates
      Then "new_file.rs" appears in the file tree

    Scenario: External file creation inside a subdirectory is reflected
      Given the src/ directory is expanded in the tree
      When a new file "helper.rs" is created externally in the src/ directory
      And the file tree processes pending updates
      Then "helper.rs" appears under src/ in the file tree

  @file-tree @file-watching @happy-path
  Rule: A file deleted externally disappears from the tree

    Scenario: External file deletion is reflected in the tree
      Given the file tree shows the project root
      When "Cargo.toml" is deleted externally
      And the file tree processes pending updates
      Then "Cargo.toml" no longer appears in the file tree

  @file-tree @file-watching @happy-path
  Rule: A new directory created externally appears in the tree

    Scenario: External directory creation is reflected in the tree
      Given the file tree shows the project root
      When a new directory "docs/" is created externally in the root directory
      And the file tree processes pending updates
      Then "docs/" appears in the file tree

  @file-tree @file-watching @happy-path
  Rule: Grandchild paths remain correct after a parent directory rescan

    Scenario: Opening a file in an expanded subdirectory uses the correct path after root rescan
      Given the src/ directory is expanded in the tree
      When a new file "trigger.rs" is created externally in the root directory
      And the file tree processes pending updates
      Then the resolved path for "main.rs" ends with "src/main.rs"

  @file-tree @file-watching @happy-path
  Rule: Rapid external changes are coalesced into a single refresh

    Scenario: Multiple rapid changes result in one tree refresh
      Given the file tree shows the project root
      When 5 files are created externally in rapid succession in the root directory
      And the file tree processes pending updates
      Then all 5 new files appear in the file tree
