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


Status
------

The program is in a well progressed development phase. A lot of the
desired functionality exists, but not all is hooked up with the UI. More
improvements are being worked on.

**notnow** typically compiles with the most recent version of stable
Rust. On compile errors please try upgrading to a more recent version
first.

[rust-lang]: https://www.rust-lang.org
[rfc-4791]: https://tools.ietf.org/html/rfc4791
[gui]: https://crates.io/crates/gui
