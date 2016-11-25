use std::thread::spawn;
use std::mem::transmute;
use std::sync::mpsc::{Sender, SendError, Receiver, RecvError, channel};

#[cfg(feature = "chan_select")]
use std::sync::mpsc::Select;
#[cfg(feature = "chan_select")]
use std::collections::HashMap;

use super::{ChannelSend, ChannelRecv, Carrier, HasDual, Chan};

pub struct Channel {
    tx: Sender<Box<u8>>,
    rx: Receiver<Box<u8>>,
}

pub struct Value<T>(pub T) where T: Send + 'static;

impl<T> ChannelSend for Value<T> where T: Send + 'static {
    type Crr = Channel;
    type Err = SendError<Box<T>>;

    fn send(self, carrier: &mut Self::Crr) -> Result<(), Self::Err> {
        unsafe {
            let tx: &Sender<Box<T>> = transmute(&carrier.tx);
            tx.send(Box::new(self.0))
        }
    }
}

impl<T> ChannelRecv for Value<T> where T: Sized + Send + 'static {
    type Crr = Channel;
    type Err = RecvError;

    fn recv(carrier: &mut Self::Crr) -> Result<Self, Self::Err> {
        unsafe {
            let rx: &Receiver<Box<T>> = transmute(&carrier.rx);
            rx.recv().map(|v| Value(*v))
        }
    }
}

impl Carrier for Channel {
    type SendChoiceErr = SendError<Box<bool>>;
    fn send_choice(&mut self, choice: bool) -> Result<(), Self::SendChoiceErr> {
        Value(choice).send(self)
    }

    type RecvChoiceErr = RecvError;
    fn recv_choice(&mut self) -> Result<bool, Self::RecvChoiceErr> {
        Value::recv(self).map(|Value(value)| value)
    }
}

/// Returns two session channels
#[must_use]
pub fn session_channel<P: HasDual>() -> (Chan<Channel, (), P>, Chan<Channel, (), P::Dual>) {
    let (master_tx, slave_rx) = channel();
    let (slave_tx, master_rx) = channel();

    let master_carrier = Channel {
        tx: master_tx,
        rx: master_rx,
    };
    let slave_carrier = Channel {
        tx: slave_tx,
        rx: slave_rx,
    };

    (Chan::new(master_carrier),
     Chan::new(slave_carrier))
}

/// Connect two functions using a session typed channel.
pub fn connect<FM, FS, P>(master_fn: FM, slave_fn: FS) where
    FM: Fn(Chan<Channel, (), P>) + Send,
    FS: Fn(Chan<Channel, (), P::Dual>) + Send + 'static,
    P: HasDual + Send + 'static,
    <P as HasDual>::Dual: HasDual + Send + 'static
{
    let (master, slave) = session_channel();
    let thread = spawn(move || slave_fn(slave));
    master_fn(master);
    thread.join().unwrap();
}

// /// Homogeneous select. We have a vector of channels, all obeying the same
// /// protocol (and in the exact same point of the protocol), wait for one of them
// /// to receive. Removes the receiving channel from the vector and returns both
// /// the channel and the new vector.
// #[cfg(feature = "chan_select")]
// #[must_use]
// pub fn hselect<E, P, A>(mut chans: Vec<Chan<E, Recv<A, P>>>)
//                         -> (Chan<E, Recv<A, P>>, Vec<Chan<E, Recv<A, P>>>)
// {
//     let i = iselect(&chans);
//     let c = chans.remove(i);
//     (c, chans)
// }

// /// An alternative version of homogeneous select, returning the index of the Chan
// /// that is ready to receive.
// #[cfg(feature = "chan_select")]
// pub fn iselect<E, P, A>(chans: &Vec<Chan<E, Recv<A, P>>>) -> usize {
//     let mut map = HashMap::new();

//     let id = {
//         let sel = Select::new();
//         let mut handles = Vec::with_capacity(chans.len()); // collect all the handles

//         for (i, chan) in chans.iter().enumerate() {
//             let &Chan(_, ref rx, _) = chan;
//             let handle = sel.handle(rx);
//             map.insert(handle.id(), i);
//             handles.push(handle);
//         }

//         for handle in handles.iter_mut() { // Add
//             unsafe { handle.add(); }
//         }

//         let id = sel.wait();

//         for handle in handles.iter_mut() { // Clean up
//             unsafe { handle.remove(); }
//         }

//         id
//     };
//     map.remove(&id).unwrap()
// }

// /// Heterogeneous selection structure for channels
// ///
// /// This builds a structure of channels that we wish to select over. This is
// /// structured in a way such that the channels selected over cannot be
// /// interacted with (consumed) as long as the borrowing ChanSelect object
// /// exists. This is necessary to ensure memory safety.
// ///
// /// The type parameter T is a return type, ie we store a value of some type T
// /// that is returned in case its associated channels is selected on `wait()`
// #[cfg(feature = "chan_select")]
// pub struct ChanSelect<'c, T> {
//     chans: Vec<(&'c Chan<(), ()>, T)>,
// }

// #[cfg(feature = "chan_select")]
// impl<'c, T> ChanSelect<'c, T> {
//     pub fn new() -> ChanSelect<'c, T> {
//         ChanSelect {
//             chans: Vec::new()
//         }
//     }

//     /// Add a channel whose next step is `Recv`
//     ///
//     /// Once a channel has been added it cannot be interacted with as long as it
//     /// is borrowed here (by virtue of borrow checking and lifetimes).
//     pub fn add_recv_ret<E, P, A: marker::Send>(&mut self,
//                                                chan: &'c Chan<E, Recv<A, P>>,
//                                                ret: T)
//     {
//         self.chans.push((unsafe { transmute(chan) }, ret));
//     }

//     pub fn add_offer_ret<E, P, Q>(&mut self,
//                                   chan: &'c Chan<E, Offer<P, Q>>,
//                                   ret: T)
//     {
//         self.chans.push((unsafe { transmute(chan) }, ret));
//     }

//     /// Find a Receiver (and hence a Chan) that is ready to receive.
//     ///
//     /// This method consumes the ChanSelect, freeing up the borrowed Receivers
//     /// to be consumed.
//     pub fn wait(self) -> T {
//         let sel = Select::new();
//         let mut handles = Vec::with_capacity(self.chans.len());
//         let mut map = HashMap::new();

//         for (chan, ret) in self.chans.into_iter() {
//             let &Chan(_, ref rx, _) = chan;
//             let h = sel.handle(rx);
//             let id = h.id();
//             map.insert(id, ret);
//             handles.push(h);
//         }

//         for handle in handles.iter_mut() {
//             unsafe { handle.add(); }
//         }

//         let id = sel.wait();

//         for handle in handles.iter_mut() {
//             unsafe { handle.remove(); }
//         }
//         map.remove(&id).unwrap()
//     }

//     /// How many channels are there in the structure?
//     pub fn len(&self) -> usize {
//         self.chans.len()
//     }
// }

// /// Default use of ChanSelect works with usize and returns the index
// /// of the selected channel. This is also the implementation used by
// /// the `chan_select!` macro.
// #[cfg(feature = "chan_select")]
// impl<'c> ChanSelect<'c, usize> {
//     pub fn add_recv<E, P, A: marker::Send>(&mut self,
//                                            c: &'c Chan<E, Recv<A, P>>)
//     {
//         let index = self.chans.len();
//         self.add_recv_ret(c, index);
//     }

//     pub fn add_offer<E, P, Q>(&mut self,
//                               c: &'c Chan<E, Offer<P, Q>>)
//     {
//         let index = self.chans.len();
//         self.add_offer_ret(c, index);
//     }
// }

// /// This macro plays the same role as the `select!` macro does for `Receiver`s.
// ///
// /// It also supports a second form with `Offer`s (see the example below).
// ///
// /// # Examples
// ///
// /// ```rust
// /// #[macro_use] extern crate session_types_ng;
// /// use session_types_ng::*;
// /// use std::thread::spawn;
// ///
// /// fn send_str(c: Chan<(), Send<String, Eps>>) {
// ///     c.send("Hello, World!".to_string()).close();
// /// }
// ///
// /// fn send_usize(c: Chan<(), Send<usize, Eps>>) {
// ///     c.send(42).close();
// /// }
// ///
// /// fn main() {
// ///     let (tcs, rcs) = session_channel();
// ///     let (tcu, rcu) = session_channel();
// ///
// ///     // Spawn threads
// ///     spawn(move|| send_str(tcs));
// ///     spawn(move|| send_usize(tcu));
// ///
// ///     chan_select! {
// ///         (c, s) = rcs.recv() => {
// ///             assert_eq!("Hello, World!".to_string(), s);
// ///             c.close();
// ///             rcu.recv().0.close();
// ///         },
// ///         (c, i) = rcu.recv() => {
// ///             assert_eq!(42, i);
// ///             c.close();
// ///             rcs.recv().0.close();
// ///         }
// ///     }
// /// }
// /// ```
// ///
// /// ```rust
// /// #![feature(rand)]
// /// #[macro_use]
// /// extern crate session_types_ng;
// /// extern crate rand;
// ///
// /// use std::thread::spawn;
// /// use session_types_ng::*;
// ///
// /// type Igo = Choose<Send<String, Eps>, Send<u64, Eps>>;
// /// type Ugo = Offer<Recv<String, Eps>, Recv<u64, Eps>>;
// ///
// /// fn srv(chan_one: Chan<(), Ugo>, chan_two: Chan<(), Ugo>) {
// ///     let _ign;
// ///     chan_select! {
// ///         _ign = chan_one.offer() => {
// ///             String => {
// ///                 let (c, s) = chan_one.recv();
// ///                 assert_eq!("Hello, World!".to_string(), s);
// ///                 c.close();
// ///                 match chan_two.offer() {
// ///                     Left(c) => c.recv().0.close(),
// ///                     Right(c) => c.recv().0.close(),
// ///                 }
// ///             },
// ///             Number => {
// ///                 chan_one.recv().0.close();
// ///                 match chan_two.offer() {
// ///                     Left(c) => c.recv().0.close(),
// ///                     Right(c) => c.recv().0.close(),
// ///                 }
// ///             }
// ///         },
// ///         _ign = chan_two.offer() => {
// ///             String => {
// ///                 chan_two.recv().0.close();
// ///                 match chan_one.offer() {
// ///                     Left(c) => c.recv().0.close(),
// ///                     Right(c) => c.recv().0.close(),
// ///                 }
// ///             },
// ///             Number => {
// ///                 chan_two.recv().0.close();
// ///                 match chan_one.offer() {
// ///                     Left(c) => c.recv().0.close(),
// ///                     Right(c) => c.recv().0.close(),
// ///                 }
// ///             }
// ///         }
// ///     }
// /// }
// ///
// /// fn cli(c: Chan<(), Igo>) {
// ///     c.sel1().send("Hello, World!".to_string()).close();
// /// }
// ///
// /// fn main() {
// ///     let (ca1, ca2) = session_channel();
// ///     let (cb1, cb2) = session_channel();
// ///
// ///     cb2.sel2().send(42).close();
// ///
// ///     spawn(move|| cli(ca2));
// ///
// ///     srv(ca1, cb1);
// /// }
// /// ```
// #[cfg(features = "chan_select")]
// #[macro_export]
// macro_rules! chan_select {
//     (
//         $(($c:ident, $name:pat) = $rx:ident.recv() => $code:expr),+
//     ) => ({
//         let index = {
//             let mut sel = $crate::ChanSelect::new();
//             $( sel.add_recv(&$rx); )+
//             sel.wait()
//         };
//         let mut i = 0;
//         $( if index == { i += 1; i - 1 } { let ($c, $name) = $rx.recv(); $code }
//            else )+
//         { unreachable!() }
//     });

//     (
//         $($res:ident = $rx:ident.offer() => { $($t:tt)+ }),+
//     ) => ({
//         let index = {
//             let mut sel = $crate::ChanSelect::new();
//             $( sel.add_offer(&$rx); )+
//             sel.wait()
//         };
//         let mut i = 0;
//         $( if index == { i += 1; i - 1 } { $res = offer!{ $rx, $($t)+ } } else )+
//         { unreachable!() }
//     })
// }
