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
