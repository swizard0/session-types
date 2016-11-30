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

## Overview ##

Consider the following channel type:

```rust
pub struct Chan<SR, E, P> {
    carrier: SR,
    session: Session<E, P>,
}
```

This describes one abstract endpoint of two interconnected channels where all underlying physical interaction is performed using `SR` carrier, and communication protocol is modelled by `Session<E, P>` type.

The session type is pure zero size phantom type (with no runtime impact):

```rust
pub struct Session<E, P>(PhantomData<(E, P)>);
```

Here parameter `P` describes a protocol that session parametrized channel must obey, and `E` encodes a current environment which is useful to implement recursion.

An object of `Chan` type always has unique set of methods, which fully depend on current `Session`. Even more, every such method requires passing `self` by move, and usually returns back the same channel but with modified `Session`. For example, consider the simplest protocol `End`, which would mean the end of communication session:

```rust
pub struct End;

impl<SR, E> Chan<SR, E, End> {
    pub fn close(self) { ... }
    pub fn shutdown(self) -> SR { ... }
}
```

Note that both these methods are implemented only for `Chan` types that fix protocol `P` parameter to `End`. In practice it means that you can `close` a channel only when it have reached a termination point in the session schema.

There are some more types in `session-types-ng` besides `End`, which could be used to describe session protocol. The basic are `Send` and `Recv`:

```rust
pub struct Send<T, P>(PhantomData<(T, P)>);

impl<SR, E, P, T> Chan<SR, E, Send<T, P>> where ... {
     pub fn send(self, v: T) -> Result<Chan<SR, E, P>, ...> { ... }
}

pub struct Recv<T, P>(PhantomData<(T, P)>);

impl<SR, E, P, T> Chan<SR, E, Recv<T, P>> where ... {
    pub fn recv(self) -> Result<(Chan<SR, E, P>, T), ...> {
}
```

These both session types have two parameters: `T`, which is the type of value we want to send or receive, and `P` which is the next protocol for our channel to continue with. Note that corresponding methods consume `self` of type `Send<T, P>`, and return the same object with modified session protocol `P`. For example, channels with session of type `Send<T, End>` would have method `send` returning an object with session set to `End`, thus losing method `send` but obtaining method `close` instead. Consider the following code snippet:

```rust
fn send_42(channel: Chan<mpsc::Channel, (), Send<mpsc::Value<usize>, End>) {
    channel
        .send(mpsc::Value(42))
        .unwrap()
        .close();
}
```

The function `send_42` accepts a channel argument whose type defines the following facts:

* It uses `std::sync::mpsc` as underlying carrier for transmission.
* The session protocol requires that we have to send a value `mpsc::Value<usize>` and then we are forced to terminate the session.

The only way to write an implementation is something like we have in the snippet above. Given this variable `channel`, the only thing we can do with it is to invoke `send` method, because this is the only method that is implemented for such type `Chan<..., Send<..>>`. Even more, after using `send` we will completely lose the object, because it will be moved into the method, and Rust will forbid to use it anymore. But instead we'll get another channel of session type `End`, and the only thing we can do with it is to `close`!
