notnow
======

**notnow** is a terminal based TODO management program (that's right,
yet another). It uses JSON for plain-text storage of a TODO database and
is conceived in the [Rust programming language][rust-lang].

One of its overarching goals is to allow tag based filtering of tasks,
along with fully user-definable tags and queries. While filtering based
on tags is already implemented, not all aspects of it are available
through the UI.

The program also acts as the first play ground for the [gui crate][gui],
which explores the design space of UI applications using Rust.


Status
------

The program is in a well progressed development phase. A lot of the
desired functionality exists, but not all is hooked up with the UI. More
improvements are being worked on.

[rust-lang]: https://www.rust-lang.org
[gui]: https://crates.io/crates/gui
