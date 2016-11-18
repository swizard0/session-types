//! session_types_base
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
//! extern crate session_types_base;
//! use session_types_base::*;
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

use std::marker;
use std::thread::spawn;
use std::mem::transmute;
use std::sync::mpsc::{Sender, Receiver, channel};
use std::marker::PhantomData;

#[cfg(feature = "chan_select")]
use std::sync::mpsc::Select;
#[cfg(feature = "chan_select")]
use std::collections::HashMap;

pub use Branch::*;

/// A session typed channel. `P` is the protocol and `E` is the environment,
/// containing potential recursion targets
#[must_use]
pub struct Chan<E, P> (Sender<Box<u8>>, Receiver<Box<u8>>, PhantomData<(E, P)>);

unsafe fn write_chan<A: marker::Send + 'static, E, P>
    (&Chan(ref tx, _, _): &Chan<E, P>, x: A)
{
    let tx: &Sender<Box<A>> = transmute(tx);
    tx.send(Box::new(x)).unwrap();
}

unsafe fn read_chan<A: marker::Send + 'static, E, P>
    (&Chan(_, ref rx, _): &Chan<E, P>) -> A
{
    let rx: &Receiver<Box<A>> = transmute(rx);
    *rx.recv().unwrap()
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

unsafe impl <A, P: HasDual> HasDual for Send<A, P> {
    type Dual = Recv<A, P::Dual>;
}

unsafe impl <A, P: HasDual> HasDual for Recv<A, P> {
    type Dual = Send<A, P::Dual>;
}

unsafe impl <P: HasDual, Q: HasDual> HasDual for Choose<P, Q> {
    type Dual = Offer<P::Dual, Q::Dual>;
}

unsafe impl <P: HasDual, Q: HasDual> HasDual for Offer<P, Q> {
    type Dual = Choose<P::Dual, Q::Dual>;
}

unsafe impl HasDual for Var<Z> {
    type Dual = Var<Z>;
}

unsafe impl <N> HasDual for Var<S<N>> {
    type Dual = Var<S<N>>;
}

unsafe impl <P: HasDual> HasDual for Rec<P> {
    type Dual = Rec<P::Dual>;
}

pub enum Branch<L, R> {
    Left(L),
    Right(R)
}

impl <E, P> Drop for Chan<E, P> {
    fn drop(&mut self) {
        panic!("Session channel prematurely dropped");
    }
}

impl<E> Chan<E, Eps> {
    /// Close a channel. Should always be used at the end of your program.
    pub fn close(mut self) {
        // This method cleans up the channel without running the panicky destructor
        // In essence, it calls the drop glue bypassing the `Drop::drop` method
        use std::mem;

        // Create some dummy values to place the real things inside
        // This is safe because nobody will read these
        // mem::swap uses a similar technique (also paired with `forget()`)
        let mut sender = unsafe { mem::uninitialized() };
        let mut receiver = unsafe { mem::uninitialized() };

        // Extract the internal sender/receiver so that we can drop them
        // We cannot drop directly since moving out of a type
        // that implements `Drop` is disallowed
        mem::swap(&mut self.0, &mut sender);
        mem::swap(&mut self.1, &mut receiver);

        drop(sender);drop(receiver); // drop them

        // Ensure Chan destructors don't run so that we don't panic
        // This also ensures that the uninitialized values don't get
        // read at any point
        mem::forget(self);
    }
}

impl<E, P, A: marker::Send + 'static> Chan<E, Send<A, P>> {
    /// Send a value of type `A` over the channel. Returns a channel with
    /// protocol `P`
    #[must_use]
    pub fn send(self, v: A) -> Chan<E, P> {
        unsafe {
            write_chan(&self, v);
            transmute(self)
        }
    }
}

impl<E, P, A: marker::Send + 'static> Chan<E, Recv<A, P>> {
    /// Receives a value of type `A` from the channel. Returns a tuple
    /// containing the resulting channel and the received value.
    #[must_use]
    pub fn recv(self) -> (Chan<E, P>, A) {
        unsafe {
            let v = read_chan(&self);
            (transmute(self), v)
        }
    }
}

impl<E, P, Q> Chan<E, Choose<P, Q>> {
    /// Perform an active choice, selecting protocol `P`.
    #[must_use]
    pub fn sel1(self) -> Chan<E, P> {
        unsafe {
            write_chan(&self, true);
            transmute(self)
        }
    }

    /// Perform an active choice, selecting protocol `Q`.
    #[must_use]
    pub fn sel2(self) -> Chan<E, Q> {
        unsafe {
            write_chan(&self, false);
            transmute(self)
        }
    }
}

/// Convenience function. This is identical to `.sel2()`
impl<Z, A, B> Chan<Z, Choose<A, B>> {
    #[must_use]
    pub fn skip(self) -> Chan<Z, B> {
        self.sel2()
    }
}

/// Convenience function. This is identical to `.sel2().sel2()`
impl<Z, A, B, C> Chan<Z, Choose<A, Choose<B, C>>> {
    #[must_use]
    pub fn skip2(self) -> Chan<Z, C> {
        self.sel2().sel2()
    }
}

/// Convenience function. This is identical to `.sel2().sel2().sel2()`
impl<Z, A, B, C, D> Chan<Z, Choose<A, Choose<B, Choose<C, D>>>> {
    #[must_use]
    pub fn skip3(self) -> Chan<Z, D> {
        self.sel2().sel2().sel2()
    }
}

/// Convenience function. This is identical to `.sel2().sel2().sel2().sel2()`
impl<Z, A, B, C, D, E> Chan<Z, Choose<A, Choose<B, Choose<C, Choose<D, E>>>>> {
    #[must_use]
    pub fn skip4(self) -> Chan<Z, E> {
        self.sel2().sel2().sel2().sel2()
    }
}

/// Convenience function. This is identical to `.sel2().sel2().sel2().sel2().sel2()`
impl<Z, A, B, C, D, E, F> Chan<Z, Choose<A, Choose<B, Choose<C, Choose<D,
                          Choose<E, F>>>>>> {
    #[must_use]
    pub fn skip5(self) -> Chan<Z, F> {
        self.sel2().sel2().sel2().sel2().sel2()
    }
}

/// Convenience function.
impl<Z, A, B, C, D, E, F, G> Chan<Z, Choose<A, Choose<B, Choose<C, Choose<D,
                             Choose<E, Choose<F, G>>>>>>> {
    #[must_use]
    pub fn skip6(self) -> Chan<Z, G> {
        self.sel2().sel2().sel2().sel2().sel2().sel2()
    }
}

/// Convenience function.
impl<Z, A, B, C, D, E, F, G, H> Chan<Z, Choose<A, Choose<B, Choose<C, Choose<D,
                                        Choose<E, Choose<F, Choose<G, H>>>>>>>> {
    #[must_use]
    pub fn skip7(self) -> Chan<Z, H> {
        self.sel2().sel2().sel2().sel2().sel2().sel2().sel2()
    }
}

impl<E, P, Q> Chan<E, Offer<P, Q>> {
    /// Passive choice. This allows the other end of the channel to select one
    /// of two options for continuing the protocol: either `P` or `Q`.
    #[must_use]
    pub fn offer(self) -> Branch<Chan<E, P>, Chan<E, Q>> {
        unsafe {
            let b = read_chan(&self);
            if b {
                Left(transmute(self))
            } else {
                Right(transmute(self))
            }
        }
    }
}

impl<E, P> Chan<E, Rec<P>> {
    /// Enter a recursive environment, putting the current environment on the
    /// top of the environment stack.
    #[must_use]
    pub fn enter(self) -> Chan<(P, E), P> {
        unsafe { transmute(self) }
    }
}

impl<E, P> Chan<(P, E), Var<Z>> {
    /// Recurse to the environment on the top of the environment stack.
    #[must_use]
    pub fn zero(self) -> Chan<(P, E), P> {
        unsafe { transmute(self) }
    }
}

impl<E, P, N> Chan<(P, E), Var<S<N>>> {
    /// Pop the top environment from the environment stack.
    #[must_use]
    pub fn succ(self) -> Chan<E, Var<N>> {
        unsafe { transmute(self) }
    }
}

/// Homogeneous select. We have a vector of channels, all obeying the same
/// protocol (and in the exact same point of the protocol), wait for one of them
/// to receive. Removes the receiving channel from the vector and returns both
/// the channel and the new vector.
#[cfg(feature = "chan_select")]
#[must_use]
pub fn hselect<E, P, A>(mut chans: Vec<Chan<E, Recv<A, P>>>)
                        -> (Chan<E, Recv<A, P>>, Vec<Chan<E, Recv<A, P>>>)
{
    let i = iselect(&chans);
    let c = chans.remove(i);
    (c, chans)
}

/// An alternative version of homogeneous select, returning the index of the Chan
/// that is ready to receive.
#[cfg(feature = "chan_select")]
pub fn iselect<E, P, A>(chans: &Vec<Chan<E, Recv<A, P>>>) -> usize {
    let mut map = HashMap::new();

    let id = {
        let sel = Select::new();
        let mut handles = Vec::with_capacity(chans.len()); // collect all the handles

        for (i, chan) in chans.iter().enumerate() {
            let &Chan(_, ref rx, _) = chan;
            let handle = sel.handle(rx);
            map.insert(handle.id(), i);
            handles.push(handle);
        }

        for handle in handles.iter_mut() { // Add
            unsafe { handle.add(); }
        }

        let id = sel.wait();

        for handle in handles.iter_mut() { // Clean up
            unsafe { handle.remove(); }
        }

        id
    };
    map.remove(&id).unwrap()
}

/// Heterogeneous selection structure for channels
///
/// This builds a structure of channels that we wish to select over. This is
/// structured in a way such that the channels selected over cannot be
/// interacted with (consumed) as long as the borrowing ChanSelect object
/// exists. This is necessary to ensure memory safety.
///
/// The type parameter T is a return type, ie we store a value of some type T
/// that is returned in case its associated channels is selected on `wait()`
#[cfg(feature = "chan_select")]
pub struct ChanSelect<'c, T> {
    chans: Vec<(&'c Chan<(), ()>, T)>,
}

#[cfg(feature = "chan_select")]
impl<'c, T> ChanSelect<'c, T> {
    pub fn new() -> ChanSelect<'c, T> {
        ChanSelect {
            chans: Vec::new()
        }
    }

    /// Add a channel whose next step is `Recv`
    ///
    /// Once a channel has been added it cannot be interacted with as long as it
    /// is borrowed here (by virtue of borrow checking and lifetimes).
    pub fn add_recv_ret<E, P, A: marker::Send>(&mut self,
                                               chan: &'c Chan<E, Recv<A, P>>,
                                               ret: T)
    {
        self.chans.push((unsafe { transmute(chan) }, ret));
    }

    pub fn add_offer_ret<E, P, Q>(&mut self,
                                  chan: &'c Chan<E, Offer<P, Q>>,
                                  ret: T)
    {
        self.chans.push((unsafe { transmute(chan) }, ret));
    }

    /// Find a Receiver (and hence a Chan) that is ready to receive.
    ///
    /// This method consumes the ChanSelect, freeing up the borrowed Receivers
    /// to be consumed.
    pub fn wait(self) -> T {
        let sel = Select::new();
        let mut handles = Vec::with_capacity(self.chans.len());
        let mut map = HashMap::new();

        for (chan, ret) in self.chans.into_iter() {
            let &Chan(_, ref rx, _) = chan;
            let h = sel.handle(rx);
            let id = h.id();
            map.insert(id, ret);
            handles.push(h);
        }

        for handle in handles.iter_mut() {
            unsafe { handle.add(); }
        }

        let id = sel.wait();

        for handle in handles.iter_mut() {
            unsafe { handle.remove(); }
        }
        map.remove(&id).unwrap()
    }

    /// How many channels are there in the structure?
    pub fn len(&self) -> usize {
        self.chans.len()
    }
}

/// Default use of ChanSelect works with usize and returns the index
/// of the selected channel. This is also the implementation used by
/// the `chan_select!` macro.
#[cfg(feature = "chan_select")]
impl<'c> ChanSelect<'c, usize> {
    pub fn add_recv<E, P, A: marker::Send>(&mut self,
                                           c: &'c Chan<E, Recv<A, P>>)
    {
        let index = self.chans.len();
        self.add_recv_ret(c, index);
    }

    pub fn add_offer<E, P, Q>(&mut self,
                              c: &'c Chan<E, Offer<P, Q>>)
    {
        let index = self.chans.len();
        self.add_offer_ret(c, index);
    }
}

/// Returns two session channels
#[must_use]
pub fn session_channel<P: HasDual>() -> (Chan<(), P>, Chan<(), P::Dual>) {
    let (tx1, rx1) = channel();
    let (tx2, rx2) = channel();

    let c1 = Chan(tx1, rx2, PhantomData);
    let c2 = Chan(tx2, rx1, PhantomData);

    (c1, c2)
}

/// Connect two functions using a session typed channel.
pub fn connect<F1, F2, P>(srv: F1, cli: F2)
    where F1: Fn(Chan<(), P>) + marker::Send + 'static,
          F2: Fn(Chan<(), P::Dual>) + marker::Send,
          P: HasDual + marker::Send + 'static,
          <P as HasDual>::Dual: HasDual + marker::Send + 'static
{
    let (c1, c2) = session_channel();
    let t = spawn(move || srv(c1));
    cli(c2);
    t.join().unwrap();
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
/// #[macro_use] extern crate session_types_base;
/// use session_types_base::*;
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

/// This macro plays the same role as the `select!` macro does for `Receiver`s.
///
/// It also supports a second form with `Offer`s (see the example below).
///
/// # Examples
///
/// ```rust
/// #[macro_use] extern crate session_types_base;
/// use session_types_base::*;
/// use std::thread::spawn;
///
/// fn send_str(c: Chan<(), Send<String, Eps>>) {
///     c.send("Hello, World!".to_string()).close();
/// }
///
/// fn send_usize(c: Chan<(), Send<usize, Eps>>) {
///     c.send(42).close();
/// }
///
/// fn main() {
///     let (tcs, rcs) = session_channel();
///     let (tcu, rcu) = session_channel();
///
///     // Spawn threads
///     spawn(move|| send_str(tcs));
///     spawn(move|| send_usize(tcu));
///
///     chan_select! {
///         (c, s) = rcs.recv() => {
///             assert_eq!("Hello, World!".to_string(), s);
///             c.close();
///             rcu.recv().0.close();
///         },
///         (c, i) = rcu.recv() => {
///             assert_eq!(42, i);
///             c.close();
///             rcs.recv().0.close();
///         }
///     }
/// }
/// ```
///
/// ```rust
/// #![feature(rand)]
/// #[macro_use]
/// extern crate session_types_base;
/// extern crate rand;
///
/// use std::thread::spawn;
/// use session_types_base::*;
///
/// type Igo = Choose<Send<String, Eps>, Send<u64, Eps>>;
/// type Ugo = Offer<Recv<String, Eps>, Recv<u64, Eps>>;
///
/// fn srv(chan_one: Chan<(), Ugo>, chan_two: Chan<(), Ugo>) {
///     let _ign;
///     chan_select! {
///         _ign = chan_one.offer() => {
///             String => {
///                 let (c, s) = chan_one.recv();
///                 assert_eq!("Hello, World!".to_string(), s);
///                 c.close();
///                 match chan_two.offer() {
///                     Left(c) => c.recv().0.close(),
///                     Right(c) => c.recv().0.close(),
///                 }
///             },
///             Number => {
///                 chan_one.recv().0.close();
///                 match chan_two.offer() {
///                     Left(c) => c.recv().0.close(),
///                     Right(c) => c.recv().0.close(),
///                 }
///             }
///         },
///         _ign = chan_two.offer() => {
///             String => {
///                 chan_two.recv().0.close();
///                 match chan_one.offer() {
///                     Left(c) => c.recv().0.close(),
///                     Right(c) => c.recv().0.close(),
///                 }
///             },
///             Number => {
///                 chan_two.recv().0.close();
///                 match chan_one.offer() {
///                     Left(c) => c.recv().0.close(),
///                     Right(c) => c.recv().0.close(),
///                 }
///             }
///         }
///     }
/// }
///
/// fn cli(c: Chan<(), Igo>) {
///     c.sel1().send("Hello, World!".to_string()).close();
/// }
///
/// fn main() {
///     let (ca1, ca2) = session_channel();
///     let (cb1, cb2) = session_channel();
///
///     cb2.sel2().send(42).close();
///
///     spawn(move|| cli(ca2));
///
///     srv(ca1, cb1);
/// }
/// ```
#[cfg(features = "chan_select")]
#[macro_export]
macro_rules! chan_select {
    (
        $(($c:ident, $name:pat) = $rx:ident.recv() => $code:expr),+
    ) => ({
        let index = {
            let mut sel = $crate::ChanSelect::new();
            $( sel.add_recv(&$rx); )+
            sel.wait()
        };
        let mut i = 0;
        $( if index == { i += 1; i - 1 } { let ($c, $name) = $rx.recv(); $code }
           else )+
        { unreachable!() }
    });

    (
        $($res:ident = $rx:ident.offer() => { $($t:tt)+ }),+
    ) => ({
        let index = {
            let mut sel = $crate::ChanSelect::new();
            $( sel.add_offer(&$rx); )+
            sel.wait()
        };
        let mut i = 0;
        $( if index == { i += 1; i - 1 } { $res = offer!{ $rx, $($t)+ } } else )+
        { unreachable!() }
    })
}
