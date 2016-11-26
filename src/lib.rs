//! session_types_ng
//!
//! This is an implementation of *session types* in Rust.
//! ```

#![cfg_attr(feature = "chan_select", feature(mpsc_select))]

use std::marker::PhantomData;

pub mod mpsc;

/// In order to support sending via session channel a value
/// should implement `ChannelSend` trait.
pub trait ChannelSend {
    type Crr;
    type Err;

    fn send(self, carrier: &mut Self::Crr) -> Result<(), Self::Err>;
}

/// In order to support receiving via session channel a value
/// should implement `ChannelRecv` trait.
pub trait ChannelRecv: Sized {
    type Crr;
    type Err;

    fn recv(carrier: &mut Self::Crr) -> Result<Self, Self::Err>;
}

pub trait Carrier: Sized {
    type SendChoiceErr;
    fn send_choice(&mut self, choice: bool) -> Result<(), Self::SendChoiceErr>;

    type RecvChoiceErr;
    fn recv_choice(&mut self) -> Result<bool, Self::RecvChoiceErr>;
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
pub struct S<N>(PhantomData<N>);

/// End of communication session
#[allow(missing_copy_implementations)]
pub struct End;

/// Receive `A`, then `P`
pub struct Recv<A, P>(PhantomData<(A, P)>);

/// Send `A`, then `P`
pub struct Send<A, P>(PhantomData<(A, P)>);

/// End of a list
#[allow(missing_copy_implementations)]
pub struct Nil;

/// More elements to follow
pub struct More<P>(PhantomData<P>);

/// Active choice between `P` and protocols in the list `L`
pub struct Choose<P, L>(PhantomData<(P, L)>);

/// Passive choice (offer) between `P` and protocols in the list `L`
pub struct Offer<P, L>(PhantomData<(P, L)>);

/// Enter a recursive environment
pub struct Rec<P>(PhantomData<P>);

/// Recurse. N indicates how many layers of the recursive environment we recurse
/// out of.
pub struct Var<N>(PhantomData<N>);

pub unsafe trait HasDual {
    type Dual;
}

unsafe impl HasDual for End {
    type Dual = End;
}

unsafe impl<A, P: HasDual> HasDual for Send<A, P> {
    type Dual = Recv<A, P::Dual>;
}

unsafe impl<A, P: HasDual> HasDual for Recv<A, P> {
    type Dual = Send<A, P::Dual>;
}

unsafe impl HasDual for Nil {
    type Dual = Nil;
}

unsafe impl<P: HasDual> HasDual for More<P> {
    type Dual = More<P::Dual>;
}

unsafe impl<P: HasDual, L: HasDual> HasDual for Choose<P, L> {
    type Dual = Offer<P::Dual, L::Dual>;
}

unsafe impl<P: HasDual, L: HasDual> HasDual for Offer<P, L> {
    type Dual = Choose<P::Dual, L::Dual>;
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

impl<E, P> Drop for Session<E, P> {
    fn drop(&mut self) {
        panic!("Session prematurely dropped");
    }
}

impl<SR, E, P> Chan<SR, E, P> {
    pub fn new(carrier: SR) -> Chan<SR, E, P> {
        Chan {
            carrier: carrier,
            session: Session(PhantomData),
        }
    }
}

impl<SR, E> Chan<SR, E, End> {
    /// Close a channel. Should always be used at the end of your program.
    pub fn close(self) {
        // This method cleans up the channel without running the panicky destructor for `Session`
        // In essence, it calls the drop glue bypassing the `Drop::drop` method
        close_chan(self);
    }
}

fn close_chan<SR, E, P>(chan: Chan<SR, E, P>) {
    drop(chan.carrier);
    std::mem::forget(chan.session);
}

fn cast_chan<SR, EA, EB, PA, PB>(chan: Chan<SR, EA, PA>) -> Chan<SR, EB, PB> {
    std::mem::forget(chan.session);
    Chan {
        carrier: chan.carrier,
        session: Session(PhantomData),
    }
}

impl<SR, E, P, T> Chan<SR, E, Send<T, P>> where SR: Carrier, T: ChannelSend<Crr = SR> {
    /// Send a value of type `T` over the channel. Returns a channel with
    /// protocol `P`
    #[must_use]
    pub fn send(mut self, v: T) -> Result<Chan<SR, E, P>, T::Err> {
        match v.send(&mut self.carrier) {
            Ok(()) =>
                Ok(cast_chan(self)),
            Err(e) => {
                close_chan(self);
                Err(e)
            },
        }
    }
}

impl<SR, E, P, T> Chan<SR, E, Recv<T, P>> where SR: Carrier, T: ChannelRecv<Crr = SR> {
    /// Receives a value of type `T` from the channel. Returns a tuple
    /// containing the resulting channel and the received value.
    #[must_use]
    pub fn recv(mut self) -> Result<(Chan<SR, E, P>, T), T::Err> {
        match <T as ChannelRecv>::recv(&mut self.carrier) {
            Ok(v) =>
                Ok((cast_chan(self), v)),
            Err(e) => {
                close_chan(self);
                Err(e)
            },
        }
    }
}

impl<SR, E, P, L> Chan<SR, E, Choose<P, L>> where SR: Carrier {
    /// Perform an active choice, selecting protocol `P` (head of the choose list).
    #[must_use]
    pub fn head(mut self) -> Result<Chan<SR, E, P>, SR::SendChoiceErr> {
        match self.carrier.send_choice(true) {
            Ok(()) =>
                Ok(cast_chan(self)),
            Err(e) => {
                close_chan(self);
                Err(e)
            },
        }
    }
}

impl<SR, E, P, Q, L> Chan<SR, E, Choose<P, More<Choose<Q, L>>>> where SR: Carrier {
     /// Perform an active choice, skipping the first element of the choose list.
    #[must_use]
    pub fn tail(mut self) -> Result<Chan<SR, E, Choose<Q, L>>, SR::SendChoiceErr> {
        match self.carrier.send_choice(false) {
            Ok(()) =>
                Ok(cast_chan(self)),
            Err(e) => {
                close_chan(self);
                Err(e)
            },
        }
    }
}

/// Convenience function. This is identical to `.tail()`
impl<SR, Z, P, Q, L> Chan<SR, Z, Choose<P, More<Choose<Q, L>>>> where SR: Carrier {
    #[must_use]
    pub fn skip(self) -> Result<Chan<SR, Z, Choose<Q, L>>, SR::SendChoiceErr> {
        self.tail()
    }
}

/// Convenience function. This is identical to `.tail().tail()`
impl<SR, Z, PA, PB, Q, L> Chan<SR, Z, Choose<PA, More<Choose<PB, More<Choose<Q, L>>>>>> where SR: Carrier {
    #[must_use]
    pub fn skip2(self) -> Result<Chan<SR, Z, Choose<Q, L>>, SR::SendChoiceErr> {
        self.tail().and_then(|c| c.tail())
    }
}

/// Convenience function. This is identical to `.tail().tail().tail()`
impl<SR, Z, PA, PB, PC, Q, L> Chan<SR, Z, Choose<PA, More<Choose<PB, More<Choose<PC, More<Choose<Q, L>>>>>>>> where SR: Carrier {
    #[must_use]
    pub fn skip3(self) -> Result<Chan<SR, Z, Choose<Q, L>>, SR::SendChoiceErr> {
        self.skip2().and_then(|c| c.tail())
    }
}

/// Convenience function. This is identical to `.tail().tail().tail().tail()`
impl<SR, Z, PA, PB, PC, PD, Q, L>
    Chan<SR, Z, Choose<PA, More<Choose<PB, More<Choose<PC, More<Choose<PD, More<Choose<Q, L>>>>>>>>>> where SR: Carrier
{
    #[must_use]
    pub fn skip4(self) -> Result<Chan<SR, Z, Choose<Q, L>>, SR::SendChoiceErr> {
        self.skip3().and_then(|c| c.tail())
    }
}

enum BranchM<SR, E, P, T> where SR: Carrier {
    Car(T),
    Cdr(Chan<SR, E, P>),
    Error(SR::RecvChoiceErr),
}

pub struct Offers<SR, E, P, T>(BranchM<SR, E, P, T>) where SR: Carrier;

impl<SR, E, P, L> Chan<SR, E, Offer<P, L>> where SR: Carrier {
    /// Passive choice. This allows the other end of the channel to navigate
    /// the given list of options.
    #[must_use]
    pub fn offer<T>(self) -> Offers<SR, E, Offer<P, L>, T> {
        Offers(BranchM::Cdr(self))
    }
}

impl<SR, E, P, Q, L, T> Offers<SR, E, Offer<P, More<Offer<Q, L>>>, T> where SR: Carrier {
    #[must_use]
    pub fn option<F>(self, mut handler: F) -> Offers<SR, E, Offer<Q, L>, T>
        where F: FnMut(Chan<SR, E, P>) -> T
    {
        match self.0 {
            BranchM::Car(value) =>
                Offers(BranchM::Car(value)),
            BranchM::Cdr(mut chan) =>
                match chan.carrier.recv_choice() {
                    Ok(true) =>
                        Offers(BranchM::Car(handler(cast_chan(chan)))),
                    Ok(false) =>
                        Offers(BranchM::Cdr(cast_chan(chan))),
                    Err(e) => {
                        close_chan(chan);
                        Offers(BranchM::Error(e))
                    },
                },
            BranchM::Error(err) =>
                Offers(BranchM::Error(err)),
        }
    }
}

impl<SR, E, P, T> Offers<SR, E, Offer<P, Nil>, T> where SR: Carrier {
    #[must_use]
    pub fn option<F>(self, mut handler: F) -> Result<T, SR::RecvChoiceErr>
        where F: FnMut(Chan<SR, E, P>) -> T
    {
        match self.0 {
            BranchM::Car(value) =>
                Ok(value),
            BranchM::Cdr(mut chan) =>
                match chan.carrier.recv_choice() {
                    Ok(true) =>
                        Ok(handler(cast_chan(chan))),
                    Ok(false) =>
                        panic!("session protocol offer list out of range"),
                    Err(e) => {
                        close_chan(chan);
                        Err(e)
                    },
                },
            BranchM::Error(err) =>
                Err(err),
        }
    }
}

impl<SR, E, P> Chan<SR, E, Rec<P>> {
    /// Enter a recursive environment, putting the current environment on the
    /// top of the environment stack.
    #[must_use]
    pub fn enter(self) -> Chan<SR, (P, E), P> {
        cast_chan(self)
    }
}

impl<SR, E, P> Chan<SR, (P, E), Var<Z>> {
    /// Recurse to the environment on the top of the environment stack.
    #[must_use]
    pub fn zero(self) -> Chan<SR, (P, E), P> {
        cast_chan(self)
    }
}

impl<SR, E, P, N> Chan<SR, (P, E), Var<S<N>>> {
    /// Pop the top environment from the environment stack.
    #[must_use]
    pub fn succ(self) -> Chan<SR, E, Var<N>> {
        cast_chan(self)
    }
}
