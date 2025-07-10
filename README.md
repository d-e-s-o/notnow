[![pipeline](https://github.com/d-e-s-o/notnow/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/d-e-s-o/notnow/actions/workflows/test.yml)
[![coverage](https://codecov.io/gh/d-e-s-o/notnow/branch/main/graph/badge.svg)](https://codecov.io/gh/d-e-s-o/notnow)
[![crates.io](https://img.shields.io/crates/v/notnow.svg)](https://crates.io/crates/notnow)
[![rustc](https://img.shields.io/badge/rustc-1.81+-blue.svg)](https://blog.rust-lang.org/2024/09/05/Rust-1.81.0/)

notnow
======

- [Changelog](CHANGELOG.md)

**notnow** is a terminal based task/TODO management program.

Two of its overarching goals are to allow for tag based filtering of
tasks, along with fully user-definable tags and views, and to support
server based synchronization of iCalendar-style TODO items as per [RFC
5545][rfc-5545] using the CalDAV protocol as specified by [RFC
4791][rfc-4791].


Status
------

**notnow** is being used actively on a day-to-day basis, without any
known issues. Most of the desired functionality exists, but not
everything is hooked up to the UI yet:

- tag based filtering is implemented and fully functionally, but not all
  aspects of it are available through the UI
- the program stores tasks as iCalendar TODO items
  - it loosely follows the [Vdir storage format][vdir-format], enabling
    task synchronization between different systems via [vdirsyncer][]
  - "native" CalDAV support without a Python dependency is considered,
    but there exist no immediate plans to get there


Configuration
-------------

The program stores its configuration below `$XDG_CONFIG_HOME/notnow/`
(which most commonly defaults to `~/.config/notnow/`). Configuration is
two-fold:
- `notnow.json` is a JSON file storing basic program state such as
  colors and views ("tabs") to display
  - when not present, defaults are being used
  - this file will be auto-created with the default contents once the
    user saves data from within the program (see below)
- `tasks/` is a folder comprised of files for individual tasks
  - the file `00000000-0000-0000-0000-000000000000` is special and
    contains task meta data such as tag information
  - the program takes care of creating said files as tasks are added

### vdirsyncer

If you would like to synchronize tasks with your CalDAV enabled calendar
and/or share one set of tasks between different systems running
**notnow**, [vdirsyncer][] is the recommended way to go about that.

Here is a configuration template that specifies relevant settings, along
with some opinionated ones (typically stored at
`$XDG_CONFIG_HOME/vdirsyncer/config`):

```ini
[general]
status_path = "~/<some-path>/vdirsyncer-state/status/"

[pair todos]
a = "todos_remote"
b = "todos_local"
collections = ["from a", "from b"]
conflict_resolution = ["command", "nvim", "-d"]

[storage todos_remote]
type = "caldav"
item_types = ["VTODO"]
url = "https://<url-to-calendar>"
username = "<username>"
password.fetch = ["command", "sh", "-c", "pass <calendar-provider-entry> | head -n1"]
read_only = false

[storage todos_local]
type = "filesystem"
path = "~/<some-path>/vdirsyncer-state/todos/"
fileext = ""
encoding = "utf-8"
```

Please refer to its [documentation][vdirsyncer-config] for additional
details.

With the configuration in place, once you `vsyncdir discover`, create a
symbolic link below `~/<some-path>/vdirsyncer-state/todos/` replacing
the automatically created subfolder with a reference to
`$XDG_CONFIG_HOME/notnow/tasks/`. Next, synchronize tasks using
`vsyncdir sync`.

Please note that task synchronization should happen before or after
running **notnow**, to prevent collisions with changes happening
concurrently from the program.


Usage
-----

Being terminal based, **notnow** is controlled through its UI as opposed
to command line parameters. The program aims to mirror Vi style bindings
where that is possible. The key bindings are as follows:

| Key(s) | Function                                 |
|--------|------------------------------------------|
| a      | Add a new task                           |
| e      | Edit selected task's summary             |
| t      | Edit selected task's tags                |
| d      | Delete selected task                     |
| y      | Copy the selected task                   |
| p      | Paste a previously copied task           |
| j      | Move task selection down                 |
| k      | Move task selection up                   |
| J      | Move selected task down                  |
| K      | Move selected task up                    |
| g      | Select first task on the current view    |
| G      | Select last task on the current view     |
| Space  | Toggle completion state of selected task |
| h      | Select view to the left                  |
| l      | Select view to the right                 |
| H      | Move view to the left                    |
| L      | Move view to the right                   |
| 1..9   | Select view #x                           |
| 0      | Select last view                         |
| `      | Select previous view                     |
| /      | Start task search forward                |
| ?      | Start task search backward               |
| n      | Continue task search forward             |
| N      | Continue task search backward            |
| *      | Start forward search for currently       |
|        | selected task on other views             |
| Return | Accept text input / Edit task details    |
| Esc    | Cancel text input                        |
| w      | Save tasks to file                       |
| q      | Quit program                             |

In addition, when inputting text (e.g., when **a**dding or **e**diting a
task), the backspace, delete, home, end, and left and right cursor keys
have functions similar to those they carry most commonly.

The program has support for [`libreadline`][libreadline] style task
input, when built with the `readline` feature flag enabled. That is,
when entering actual text (as opposed to just pressing a key to, say,
selecting a different task), `libreadline` bindings will be honored.


Example
-------

If you are just interested in trying out the program with some
programmatically created tasks, you can use the existing example:
```sh
$ cargo run --example=with-test-data --features=test
```

Note that if saved, tasks will be stored in a temporary directory and
not overwrite user-specific configuration mentioned above.


[rfc-4791]: https://tools.ietf.org/html/rfc4791
[rfc-5545]: https://www.rfc-editor.org/rfc/rfc5545
[vdir-format]: http://vdirsyncer.pimutils.org/en/stable/vdir.html
[vdirsyncer]: https://github.com/pimutils/vdirsyncer
[vdirsyncer-config]: http://vdirsyncer.pimutils.org/en/stable/index.html
[libreadline]: https://tiswww.case.edu/php/chet/readline/readline.html
