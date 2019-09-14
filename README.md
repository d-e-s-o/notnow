[![pipeline](https://gitlab.com/d-e-s-o/notnow/badges/devel/pipeline.svg)](https://gitlab.com/d-e-s-o/notnow/commits/devel)
[![crates.io](https://img.shields.io/crates/v/notnow.svg)](https://crates.io/crates/notnow)
[![rustc](https://img.shields.io/badge/rustc-1.34+-blue.svg)](https://blog.rust-lang.org/2019/04/11/Rust-1.34.0.html)

notnow
======

- [Changelog](CHANGELOG.md)

**notnow** is a terminal based TODO management program (that's right,
yet another). It uses JSON for plain-text storage of a TODO database and
is conceived in the [Rust programming language][rust-lang].

Two of its overarching goals are to allow for tag based filtering of
tasks, along with fully user-definable tags and queries, and to support
server based synchronization of tasks using the CalDAV protocol as
specified by [RFC 4791][rfc-4791].
While filtering based on tags is already implemented, not all aspects of
it are available through the UI. CalDAV support has not yet found its
way into the program.

The program also acts as the first play ground for the [gui crate][gui],
which explores the design space of UI applications using Rust.


Usage
-----

The program stores its configuration below `$XDG_CONFIG_HOME/notnow`
(which most commonly defaults to `~/.config/notnow`). Configuration is
two-fold:
- `notnow.json` is a JSON file storing basic program state
- `task.json` is a JSON file storing the user's tasks

Being terminal based, **notnow** is controlled through its UI as opposed
to command line parameters. The program aims to mirror Vi style bindings
where that is possible. The key bindings are as follows:

| Key(s) | Function                                 |
|--------|------------------------------------------|
| a      | Add a new task                           |
| e      | Edit selected task                       |
| d      | Delete selected task                     |
| j      | Move task selection down                 |
| k      | Move task selection up                   |
| J      | Move selected task down                  |
| K      | Move selected task up                    |
| g      | Select first task on the current tab     |
| G      | Select last task on the current tab      |
| Space  | Toggle completion state of selected task |
| h      | Select tab to the left                   |
| l      | Select tab to the right                  |
| 1-9    | Select tab #x                            |
| 0      | Select last tab                          |
| `      | Select previous tab                      |
| /      | Start task search forward                |
| ?      | Start task search backward               |
| n      | Continue task search forward             |
| N      | Continue task search backward            |
| Return | Accept text input                        |
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


Status
------

**notnow** is in a well progressed development phase. A lot of the
desired functionality exists, but not all is hooked up with the UI. More
improvements are being worked on.

[rust-lang]: https://www.rust-lang.org
[rfc-4791]: https://tools.ietf.org/html/rfc4791
[gui]: https://crates.io/crates/gui
[libreadline]: https://tiswww.case.edu/php/chet/readline/readline.html
