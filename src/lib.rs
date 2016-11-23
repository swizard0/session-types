//! session_types_ng
//!
//! This is an implementation of *session types* in Rust.
//!
//! The channels in Rusts standard library are useful for a great many things,
//! but they're restricted to a single type. Session types allows one to use a
//! single channel for transferring values of different types, depending on the
//! context in which it is used. Specifically, a session typed channel always
//! carry a *protocol*, which dictates how communication is to take place.
//!
//! For example, imagine that two threads, `A` and `B` want to communicate with
//! the following pattern:
//!
//!  1. `A` sends an integer to `B`.
//!  2. `B` sends a boolean to `A` depending on the integer received.
//!
//! With session types, this could be done by sharing a single channel. From
//! `A`'s point of view, it would have the type `int ! (bool ? eps)` where `t ! r`
//! is the protocol "send something of type `t` then proceed with
//! protocol `r`", the protocol `t ? r` is "receive something of type `t` then proceed
//! with protocol `r`, and `eps` is a special marker indicating the end of a
//! communication session.
//!
//! Our session type library allows the user to create channels that adhere to a
//! specified protocol. For example, a channel like the above would have the type
//! `Chan<(), Send<i64, Recv<bool, Eps>>>`, and the full program could look like this:
//!
//! ```
//! extern crate session_types_ng;
//! use session_types_ng::*;
//!
//! type Server = Recv<i64, Send<bool, Eps>>;
//! type Client = Send<i64, Recv<bool, Eps>>;
//!
//! fn srv(c: Chan<(), Server>) {
//!     let (c, n) = c.recv();
//!     if n % 2 == 0 {
//!         c.send(true).close()
//!     } else {
//!         c.send(false).close()
//!     }
//! }
//!
//! fn cli(c: Chan<(), Client>) {
//!     let n = 42;
//!     let c = c.send(n);
//!     let (c, b) = c.recv();
//!
//!     if b {
//!         println!("{} is even", n);
//!     } else {
//!         println!("{} is odd", n);
//!     }
//!
//!     c.close();
//! }
//!
//! fn main() {
//!     connect(srv, cli);
//! }
//! ```

#![cfg_attr(feature = "chan_select", feature(mpsc_select))]

use std::marker::PhantomData;

pub use Branch::*;

/// In order to support sending via session channel a value
/// should implement `ChannelSend` trait.
pub trait ChannelSend {
    type Chan;
    type Err;

    fn proceed_send(self, &mut Self::Chan) -> Result<(), Self::Err>;
}

/// In order to support receiving via session channel a value
/// should implement `ChannelRecv` trait.
pub trait ChannelRecv: Sized {
    type Chan;
    type Err;

    fn proceed_recv(&mut Self::Chan) -> Result<Self, Self::Err>;
}

pub trait Carrier: Sized {
    type Err;

    fn send<T>(&mut self, value: T) -> Result<(), Self::Err> where T: ChannelSend<Chan = Self, Err = Self::Err>;
    fn recv<T>(&mut self) -> Result<T, Self::Err> where T: ChannelRecv<Chan = Self, Err = Self::Err>;

    fn send_choice(&mut self, choice: bool) -> Result<(), Self::Err>;
    fn recv_choice(&mut self) -> Result<bool, Self::Err>;
}

/// A session for a session typed channel.
/// `P` is the protocol
/// `E` is the environment, containing potential recursion targets
pub struct Session<E, P>(PhantomData<(E, P)>);

/// A session typed channel.
/// `SR` is the carrier channel for actual sending and receiving
/// `P` is the protocol
/// `E` is the environment, containing potential recursion targets
#[must_use]
pub struct Chan<SR, E, P> {
    carrier: SR,
    session: Session<E, P>,
}

/// Peano numbers: Zero
#[allow(missing_copy_implementations)]
pub struct Z;

/// Peano numbers: Increment
pub struct S<N> ( PhantomData<N> );

/// End of communication session (epsilon)
#[allow(missing_copy_implementations)]
pub struct Eps;

/// Receive `A`, then `P`
pub struct Recv<A, P> ( PhantomData<(A, P)> );

/// Send `A`, then `P`
pub struct Send<A, P> ( PhantomData<(A, P)> );

/// Active choice between `P` and `Q`
pub struct Choose<P, Q> ( PhantomData<(P, Q)> );

/// Passive choice (offer) between `P` and `Q`
pub struct Offer<P, Q> ( PhantomData<(P, Q)> );

/// Enter a recursive environment
pub struct Rec<P> ( PhantomData<P> );

/// Recurse. N indicates how many layers of the recursive environment we recurse
/// out of.
pub struct Var<N> ( PhantomData<N> );

pub unsafe trait HasDual {
    type Dual;
}

unsafe impl HasDual for Eps {
    type Dual = Eps;
}

unsafe impl<A, P: HasDual> HasDual for Send<A, P> {
    type Dual = Recv<A, P::Dual>;
}

unsafe impl<A, P: HasDual> HasDual for Recv<A, P> {
    type Dual = Send<A, P::Dual>;
}

unsafe impl<P: HasDual, Q: HasDual> HasDual for Choose<P, Q> {
    type Dual = Offer<P::Dual, Q::Dual>;
}

unsafe impl<P: HasDual, Q: HasDual> HasDual for Offer<P, Q> {
    type Dual = Choose<P::Dual, Q::Dual>;
}

unsafe impl HasDual for Var<Z> {
    type Dual = Var<Z>;
}

unsafe impl<N> HasDual for Var<S<N>> {
    type Dual = Var<S<N>>;
}

unsafe impl<P: HasDual> HasDual for Rec<P> {
    type Dual = Rec<P::Dual>;
}

pub enum Branch<L, R> {
    Left(L),
    Right(R)
}

impl<E, P> Drop for Session<E, P> {
    fn drop(&mut self) {
        panic!("Session prematurely dropped");
    }
}

impl<SR, E> Chan<SR, E, Eps> {
    /// Close a channel. Should always be used at the end of your program.
    pub fn close(self) {
        // This method cleans up the channel without running the panicky destructor for `Session`
        // In essence, it calls the drop glue bypassing the `Drop::drop` method
        drop(self.carrier);
        std::mem::forget(self.session);
    }
}

impl<SR, E, P, T> Chan<SR, E, Send<T, P>> where SR: Carrier, T: ChannelSend<Chan = SR, Err = SR::Err> {
    /// Send a value of type `T` over the channel. Returns a channel with
    /// protocol `P`
    #[must_use]
    pub fn send(mut self, v: T) -> Result<Chan<SR, E, P>, (SR::Err, Chan<SR, E, Send<T, P>>)> {
        match self.carrier.send(v) {
            Ok(()) =>
                Ok(Chan {
                    carrier: self.carrier,
                    session: Session(PhantomData),
                }),
            Err(e) =>
                Err((e, self)),
        }
    }
}

impl<SR, E, P, T> Chan<SR, E, Recv<T, P>> where SR: Carrier, T: ChannelRecv<Chan = SR, Err = SR::Err> {
    /// Receives a value of type `T` from the channel. Returns a tuple
    /// containing the resulting channel and the received value.
    #[must_use]
    pub fn recv(mut self) -> Result<(Chan<SR, E, P>, T), (SR::Err, Chan<SR, E, Recv<T, P>>)> {
        match self.carrier.recv() {
            Ok(v) =>
                Ok((Chan {
                    carrier: self.carrier,
                    session: Session(PhantomData),
                }, v)),
            Err(e) =>
                Err((e, self)),
        }
    }
}

// impl<E, P, Q> Chan<E, Choose<P, Q>> {
//     /// Perform an active choice, selecting protocol `P`.
//     #[must_use]
//     pub fn sel1(self) -> Chan<E, P> {
//         unsafe {
//             write_chan(&self, true);
//             transmute(self)
//         }
//     }

//     /// Perform an active choice, selecting protocol `Q`.
//     #[must_use]
//     pub fn sel2(self) -> Chan<E, Q> {
//         unsafe {
//             write_chan(&self, false);
//             transmute(self)
//         }
//     }
// }

// /// Convenience function. This is identical to `.sel2()`
// impl<Z, A, B> Chan<Z, Choose<A, B>> {
//     #[must_use]
//     pub fn skip(self) -> Chan<Z, B> {
//         self.sel2()
//     }
// }

// /// Convenience function. This is identical to `.sel2().sel2()`
// impl<Z, A, B, C> Chan<Z, Choose<A, Choose<B, C>>> {
//     #[must_use]
//     pub fn skip2(self) -> Chan<Z, C> {
//         self.sel2().sel2()
//     }
// }

// /// Convenience function. This is identical to `.sel2().sel2().sel2()`
// impl<Z, A, B, C, D> Chan<Z, Choose<A, Choose<B, Choose<C, D>>>> {
//     #[must_use]
//     pub fn skip3(self) -> Chan<Z, D> {
//         self.sel2().sel2().sel2()
//     }
// }

// /// Convenience function. This is identical to `.sel2().sel2().sel2().sel2()`
// impl<Z, A, B, C, D, E> Chan<Z, Choose<A, Choose<B, Choose<C, Choose<D, E>>>>> {
//     #[must_use]
//     pub fn skip4(self) -> Chan<Z, E> {
//         self.sel2().sel2().sel2().sel2()
//     }
// }

// /// Convenience function. This is identical to `.sel2().sel2().sel2().sel2().sel2()`
// impl<Z, A, B, C, D, E, F> Chan<Z, Choose<A, Choose<B, Choose<C, Choose<D,
//                           Choose<E, F>>>>>> {
//     #[must_use]
//     pub fn skip5(self) -> Chan<Z, F> {
//         self.sel2().sel2().sel2().sel2().sel2()
//     }
// }

// /// Convenience function.
// impl<Z, A, B, C, D, E, F, G> Chan<Z, Choose<A, Choose<B, Choose<C, Choose<D,
//                              Choose<E, Choose<F, G>>>>>>> {
//     #[must_use]
//     pub fn skip6(self) -> Chan<Z, G> {
//         self.sel2().sel2().sel2().sel2().sel2().sel2()
//     }
// }

// /// Convenience function.
// impl<Z, A, B, C, D, E, F, G, H> Chan<Z, Choose<A, Choose<B, Choose<C, Choose<D,
//                                         Choose<E, Choose<F, Choose<G, H>>>>>>>> {
//     #[must_use]
//     pub fn skip7(self) -> Chan<Z, H> {
//         self.sel2().sel2().sel2().sel2().sel2().sel2().sel2()
//     }
// }

// impl<E, P, Q> Chan<E, Offer<P, Q>> {
//     /// Passive choice. This allows the other end of the channel to select one
//     /// of two options for continuing the protocol: either `P` or `Q`.
//     #[must_use]
//     pub fn offer(self) -> Branch<Chan<E, P>, Chan<E, Q>> {
//         unsafe {
//             let b = read_chan(&self);
//             if b {
//                 Left(transmute(self))
//             } else {
//                 Right(transmute(self))
//             }
//         }
//     }
// }

impl<SR, E, P> Chan<SR, E, Rec<P>> {
    /// Enter a recursive environment, putting the current environment on the
    /// top of the environment stack.
    #[must_use]
    pub fn enter(self) -> Chan<SR, (P, E), P> {
        Chan {
            carrier: self.carrier,
            session: Session(PhantomData),
        }
    }
}

impl<SR, E, P> Chan<SR, (P, E), Var<Z>> {
    /// Recurse to the environment on the top of the environment stack.
    #[must_use]
    pub fn zero(self) -> Chan<SR, (P, E), P> {
        Chan {
            carrier: self.carrier,
            session: Session(PhantomData),
        }
    }
}

impl<SR, E, P, N> Chan<SR, (P, E), Var<S<N>>> {
    /// Pop the top environment from the environment stack.
    #[must_use]
    pub fn succ(self) -> Chan<SR, E, Var<N>> {
        Chan {
            carrier: self.carrier,
            session: Session(PhantomData),
        }
    }
}

/// This macro is convenient for server-like protocols of the form:
///
/// `Offer<A, Offer<B, Offer<C, ... >>>`
///
/// # Examples
///
/// Assume we have a protocol `Offer<Recv<u64, Eps>, Offer<Recv<String, Eps>,Eps>>>`
/// we can use the `offer!` macro as follows:
///
/// ```rust
/// #[macro_use] extern crate session_types_ng;
/// use session_types_ng::*;
/// use std::thread::spawn;
///
/// fn srv(c: Chan<(), Offer<Recv<u64, Eps>, Offer<Recv<String, Eps>, Eps>>>) {
///     offer! { c,
///         Number => {
///             let (c, n) = c.recv();
///             assert_eq!(42, n);
///             c.close();
///         },
///         String => {
///             c.recv().0.close();
///         },
///         Quit => {
///             c.close();
///         }
///     }
/// }
///
/// fn cli(c: Chan<(), Choose<Send<u64, Eps>, Choose<Send<String, Eps>, Eps>>>) {
///     c.sel1().send(42).close();
/// }
///
/// fn main() {
///     let (s, c) = session_channel();
///     spawn(move|| cli(c));
///     srv(s);
/// }
/// ```
///
/// The identifiers on the left-hand side of the arrows have no semantic
/// meaning, they only provide a meaningful name for the reader.
#[macro_export]
macro_rules! offer {
    (
        $id:ident, $branch:ident => $code:expr, $($t:tt)+
    ) => (
        match $id.offer() {
            Left($id) => $code,
            Right($id) => offer!{ $id, $($t)+ }
        }
    );
    (
        $id:ident, $branch:ident => $code:expr
    ) => (
        $code
    )
}
