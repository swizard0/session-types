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

## Tutorial ##

### Channels and sessions ###

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

### Closing a channel ###

An object of `Chan` type always has unique set of methods, which fully depend on current `Session`. Even more, every such method requires passing `self` by move, and usually returns back the same channel but with modified `Session`. For example, consider the simplest protocol `End`, which would mean the end of communication session:

```rust
pub struct End;

impl<SR, E> Chan<SR, E, End> {
    pub fn close(self) { ... }
    pub fn shutdown(self) -> SR { ... }
}
```

Note that both these methods are implemented only for `Chan` types that fix protocol `P` parameter to `End`. In practice it means that you can `close` a channel only when it have reached a termination point in the session schema.

### Sending and receiving ###

There are some more types in `session-types-ng` besides `End`, which could be used to describe session protocol. The basic are `Send` and `Recv`:

```rust
pub struct Send<T, P>(PhantomData<(T, P)>);

impl<SR, E, P, T> Chan<SR, E, Send<T, P>> where ... {
     pub fn send(self, v: T) -> Result<Chan<SR, E, P>, ...> { ... }
}

pub struct Recv<T, P>(PhantomData<(T, P)>);

impl<SR, E, P, T> Chan<SR, E, Recv<T, P>> where ... {
    pub fn recv(self) -> Result<(Chan<SR, E, P>, T), ...> { ... }
}
```

These both session types have two parameters: `T`, which is the type of value we want to send or receive, and `P` which is the next protocol for our channel to continue with. Note that corresponding methods consume `self` of type `Send<T, P>`, and return the same object with modified session protocol `P`. For example, channels with session of type `Send<T, End>` would have method `send` returning an object with session set to `End`, thus losing method `send` but obtaining method `close` instead. Consider the following code snippet:

```rust
fn send_42(channel: Chan<mpsc::Channel, (), Send<mpsc::Value<usize>, End>>) {
    channel
        .send(mpsc::Value(42))
        .unwrap()
        .close();
}
```

The function `send_42` accepts a channel argument whose type defines the following facts:

* It uses `std::sync::mpsc` as underlying carrier for transmission.
* The session protocol requires that we have to send a value `mpsc::Value<usize>` and then we are forced to terminate the session.

The only way to write an implementation is something like we have in the snippet above. Given this variable `channel`, the only thing we can do with it is to invoke `send` method, because there is no other methods for such type `Chan<..., Send<..>>`. Even more, after using `send` we will completely lose the object, because it will be moved into the method, and Rust will forbid to use it anymore. But instead we'll get another channel of session type `End`, and the only thing we can do with it is to `close`!

Let's consider how a corresponding function `recv_42` should look like:

```rust
fn recv_42(channel: Chan<mpsc::Channel, (), Recv<mpsc::Value<usize>, End>>) -> usize {
    let (channel, mpsc::Value(result)) = channel.recv().unwrap();
    channel.close();
    result
}
```

The function signature is almost the same except the session protocol contains `Recv` type instead of `Send`. That makes sense: whenever a value is sent on the one endpoint of channel, it should be received on the other.

### `HasDual` trait ###

Actually every possible session type in the library has its dual type, like `Send` <-> `Recv`. We could use `HasDual` trait for this purpose, which is defined in the following way:

```rust
pub unsafe trait HasDual {
    type Dual;
}
```

And it is implemented for all session types, for example:

```rust
unsafe impl<T, P: HasDual> HasDual for Send<T, P> {
    type Dual = Recv<T, P::Dual>;
}
```

So we could take advantage of `HasDual::Dual` assosiated type and declare type alias for our `send_42` and `recv_42` functions:

```rust
type Send42Proto = Send<mpsc::Value<usize>, End>;
type Recv42Proto = <Send42Proto as HasDual>::Dual;

fn send_42(channel: Chan<mpsc::Channel, (), Send42Proto>) { ... }
fn recv_42(channel: Chan<mpsc::Channel, (), Recv42Proto>) -> usize { ... }
```

### Conditional branching ###

Sometimes during the protocol execution there is a point where we are not yet sure what to do next. For example, a request has failed, so both server and client want to close their session. But they cannot do it, because there is no `close` method available for channel in the middle of the protocol!

So there exist two dual session types for this kind of branching: active choice `Choose` for one side, and passive choice `Offer` for the other.

```rust
pub struct Choose<P, L>(PhantomData<(P, L)>);
pub struct Offer<P, L>(PhantomData<(P, L)>);
pub struct Nil;
```

They both used for describing the possible ways the protocol can proceed. Inside channel communication process looks like the following:

* an `Offer` endpoint waits for a `Choose` one to select one of the protocols available to continue with
* receives somehow the decision
* and finally installs the protocol chosen, so both endpoints of the channel is synchronized

`Choose` and `Offer` types represent a set of protocols for branching as linked list, where `Nil` stands for empty list, and `Choose<P, L>` or `Offer<P, L>` specify list node (cons cell) with protocol `P` and (recursively) the tail of the list `L`. For example, consider the following example: some protocol point where a decision should be made: either a value of type `usize` should be transmitted (see previously declared `Send42Proto`), or a session should be terminated instantly and its channel should be closed.

```rust
type SendOrCloseProto = Choose<Send42Proto, Choose<End, Nil>>;
type RecvOrCloseProto = Offer<Recv42Proto, Offer<End, Nil>>;
// or better
type RecvOrCloseProto = <SendOrCloseProto as HasDual>::Dual;
```

Every channel parametrized with `Choose` session has method `first`, which is used to pick the first protocol from the list:

```rust
impl<SR, E, P, L> Chan<SR, E, Choose<P, L>> where ... {
    pub fn first(self) -> Result<Chan<SR, E, P>, ...> { ... }
}
```

When a session has a list of two or more elements the method `second` appears:

```rust
impl<SR, E, P, Q, L> Chan<SR, E, Choose<P, Choose<Q, L>>> where ... {
    pub fn second(self) -> Result<Chan<SR, E, Q>, ...> { ... }
}
```

And so on. Note that we cannot go beyond the end of a list (such as choosing `third` on a list with only two protocols) because there would not be appropriate methods declared. This is how we could send a value through the channel with `SendOrCloseProto` session:

```rust
fn choose_send_42(channel: Chan<mpsc::Channel, (), SendOrCloseProto>) {
    send_42(channel.first().unwrap())
}
```

The opposite endpoint of the channel with `Offer` session does not know in runtime which protocol has been chosen by `Choose` side, so it has to provide a handler for each possible case. This could be done by invoking `offer` method on the channel in order to obtain `Offers` object, and then call its `option` method for every protocol in the list. Here are these methods signatures:

```rust
pub struct Offers<SR, E, P, T>(...) where ...;

impl<SR, E, P, L> Chan<SR, E, Offer<P, L>> where ... {
    pub fn offer<T>(self) -> Offers<SR, E, Offer<P, L>, T> { ... }
}

impl<SR, E, P, Q, L, T> Offers<SR, E, Offer<P, Offer<Q, L>>, T> where ... {
    pub fn option<F>(self, handler: F) -> Offers<SR, E, Offer<Q, L>, T>
        where F: FnMut(Chan<SR, E, P>) -> T
    { ... }
}

impl<SR, E, P, T> Offers<SR, E, Offer<P, Nil>, T> where ... {
    pub fn option<F>(self, handler: F) -> Result<T, ...>
        where F: FnMut(Chan<SR, E, P>) -> T
    { ... }
}
```

This could seems quite difficult to understand, but the main idea is pretty simple:

* Just provide one handler for each option.
* Each handler should receive its own channel parametrized with protocol given for this particular case.
* All handlers should return a value of the same type `T`.
* All `option` methods except the last return `Offers` object.
* An `option` method for the last examined case will return a `Result<T, ...>`.

Probably the easiest approach here to handle `offers` is just to declare a sum-type of all possible channel results and then examine them via `match`:

```rust
fn offer_recv_42(channel: Chan<mpsc::Channel, (), RecvOrCloseProto>) -> Option<usize> {
    enum Req<R, Q> {
        Recv(R),
        Close(Q),
    }

    let req = channel
        .offer()
        .option(Req::Recv)
        .option(Req::Close)
        .unwrap();

    match req {
        Req::Recv(chan) =>
            Some(recv_42(chan)),
        Req::Close(chan) => {
            chan.close();
            None
        }
    }
}
```

### Loops ###
