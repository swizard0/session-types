# session-types-ng #

## Summary ##

A little bit reinterpreted fork of original [session-types](https://github.com/Munksgaard/session-types) proof-of-concept project. You should probably check out [Session Types for Rust](http://munksgaard.me/laumann-munksgaard-larsen.pdf) paper for a more complete information about it.

The current project is slightly redesigned because it aims at more practical purpose. For example, it allows arbitrary underlying communication carrier for session channels, not only just `std::sync::mpsc`. Any `Read`+`Write` device is suitable, for instance [session-types-rmp](https://github.com/swizard0/session-types-rmp).

## Usage ##

`Cargo.toml`:

```toml
[dependencies]
session-types-ng = "0.3"
```

To `src/main.rs`:

```rust
extern crate session-types-ng;
```
