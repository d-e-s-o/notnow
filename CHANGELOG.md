Unreleased
----------
- Added support for finding the currently selected task on other tabs
  via '*'
- Re-select task after successfully editing tags
- Use Unicode aware lowercase in task summary search
- Bumped minimum supported Rust version to `1.52`
- Bumped `dirs` dependency to `4.0`


0.2.6
-----
- Introduced `undo` and `redo` functionality via 'u' and 'U'
- Added support for jumping to tags via 'f' and 'F'
- Bumped `dirs` dependency to `3.0`
- Added `tokio` dependency in version `1.8`
- Switched to using tarpaulin for code coverage collection
- Bumped minimum supported Rust version to `1.46`


0.2.5
-----
- Added support for editing tags through newly introduced dialog widget,
  accessible via 't'
- Changed placement of newly created tasks to be after currently
  selected one
- Bumped required Rust version to `1.43`


0.2.4
-----
- Reworked internal event handling logic to be `async`
- Excluded unnecessary files from being contained in release bundle
- Bumped `gui` dependency to `0.6`
- Added `async-trait` dependency in version `0.1`
- Added `tokio` dependency in version `1.0`


0.2.3
-----
- Reduce synchronization overhead by locking stdout only once
- Adjusted link to coverage to actually reference coverage information
  artifacts
- Bumped `gui` dependency to `0.5`
- Bumped `rline` dependency to `0.3`
- Bumped required Rust version to `1.42`


0.2.2
-----
- Improved support for handling multi-byte characters
- Fixed assertion failure when searching after aborting search term entry
- Lowercased `Pos` and `Neg` fields of serialized queries
- Added code coverage collection and reporting to CI pipeline
- Bumped `dirs` dependency to `2.0`


0.2.1
-----
- Added support for configuring colors through `notnow.json`
- Added support for moving tabs left/right
- Added support for creating a core dump on panic
- Bumped `gui` dependency to `0.4`
- Bumped required Rust version to `1.34`
- Downgraded `deny` crate-level lints to `warn`


0.2.0
-----
- Save and restore selected tab
- Save and restore selected task on each tab
- Support all `libreadline` supported keys when `readline` feature is
  enabled
- Automatically create configuration directory instead of potentially
  failing save operation
- Further decreased binary size by using system allocator
- Bumped `gui` dependency to `0.3`
- Bumped `rline` dependency to `0.2`


0.1.5
-----
- Fixed problem caused by input of multi-byte characters
  - They will from now on be ignored
- Updated README to reflect goals more accurately and to depict key
  bindings
- Adjusted program to use Rust Edition 2018
- Enabled `unused-results` lint
- Enabled CI pipeline comprising building, testing, and linting of the
  project
- Added badges indicating pipeline status, current `crates.io` published
  version of the crate, and minimum version of `rustc` required
- Added categories to `Cargo.toml`


0.1.4
-----
- Added support for `libreadline` controlled text input
  - Controlled through newly introduced `readline` feature
  - Added new dependency to `rline`
- Prevented unnecessary redraw operations on certain actions that set
  an `InOutArea` widget state
- Fixed assertion failure when pressing space when no tasks are present
  in the current `TaskListBox`
- Factored out `uid` crate which is now an explicit dependency


0.1.3
-----
- Removed default "all" query (very first query; capturing all tasks)
  - If still desired, can be configured manually
- Added support for searching in reverse order
  - '?' starts a reverse search
  - 'N' continues the existing search in reverse
- Added support for writing output to a file supplied by path as the
  first argument
  - E.g., notnow /dev/pts/3 will display the output on tty 3
- Fixed stack overflow due to endless loop when searching for a string
  that is not found on multiple tabs
- Fixed integer overflow when selecting the last tab by means of '0' and
  then advancing further


0.1.2
-----
- Added support for searching (and selecting) tasks via a sub-string of
  their summary
- Added support for selecting tabs by numbers
- Added support for directly selecting the previously active tab
- Editing a task summary to empty deletes the task
- Replaced deprecated `std::env::home_dir` with functionality from
  `dirs` crate
  - Added new dependency to `dirs`
- Enabled Rust 2018 edition lints
- Bumped `gui` dependency to `0.2`


0.1.1
-----
- Added support for automatically redrawing the UI after terminal
  resizes


0.1.0
-----
- Initial release
