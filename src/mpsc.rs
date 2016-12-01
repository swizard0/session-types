use std::thread::spawn;
use std::mem::transmute;
use std::sync::mpsc::{Sender, SendError, Receiver, RecvError, channel};
use super::{ChannelSend, ChannelRecv, Carrier, HasDual, Chan};

pub struct Channel {
    tx: Sender<Box<u8>>,
    rx: Receiver<Box<u8>>,
}

impl<T> ChannelSend<Channel> for T where T: Send + 'static {
    type Err = SendError<Box<T>>;

    fn send(self, carrier: &mut Channel) -> Result<(), Self::Err> {
        unsafe {
            let tx: &Sender<Box<T>> = transmute(&carrier.tx);
            tx.send(Box::new(self))
        }
    }
}

impl<T> ChannelRecv<Channel> for T where T: Sized + Send + 'static {
    type Err = RecvError;

    fn recv(carrier: &mut Channel) -> Result<Self, Self::Err> {
        unsafe {
            let rx: &Receiver<Box<T>> = transmute(&carrier.rx);
            rx.recv().map(|v| *v)
        }
    }
}

impl Carrier for Channel {
    type SendChoiceErr = SendError<Box<bool>>;
    fn send_choice(&mut self, choice: bool) -> Result<(), Self::SendChoiceErr> {
        choice.send(self)
    }

    type RecvChoiceErr = RecvError;
    fn recv_choice(&mut self) -> Result<bool, Self::RecvChoiceErr> {
        bool::recv(self)
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
