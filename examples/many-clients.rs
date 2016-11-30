extern crate session_types_ng;
extern crate rand;

use std::sync::mpsc::{channel, Receiver};
use std::thread::spawn;
use rand::random;

use session_types_ng::*;

type Server = Recv<u8, Choose<Send<u8, End>, Choose<End, Nil>>>;
type Client = <Server as HasDual>::Dual;

fn server_handler(chan: Chan<mpsc::Channel, (), Server>) {
    let (chan, n) = chan.recv().unwrap();
    match n.checked_add(42) {
        Some(n) => chan
            .first().unwrap()
            .send(n).unwrap()
            .close(),
        None => chan
            .second().unwrap()
            .close(),
    }
}

fn server(rx: Receiver<Chan<mpsc::Channel, (), Server>>) {
    let mut count = 0;
    loop {
        match rx.recv() {
            Ok(c) => {
                spawn(move || server_handler(c));
                count += 1;
            },
            Err(_) =>
                break,
        }
    }
    println!("Handled {} connections", count);
}

fn client_handler(chan: Chan<mpsc::Channel, (), Client>) {
    let n = random();
    chan
        .send(n).unwrap()
        .offer()
        .option(|chan_success| {
            let (chan, n2) = chan_success.recv().unwrap();
            chan.close();
            println!("{} + 42 = {}", n, n2);
        })
        .option(|chan_fail| {
            chan_fail.close();
            println!("{} + 42 is an overflow :(", n);
        })
        .unwrap();
}

fn main() {
    let (tx, rx) = channel();

    let n: u8 = random();
    println!("Spawning {} clients", n);
    for _ in 0..n {
        let tmp = tx.clone();
        spawn(move || {
            let (c1, c2) = mpsc::session_channel();
            std::sync::mpsc::Sender::<_>::send(&tmp, c1).unwrap();
            client_handler(c2);
        });
    }
    drop(tx);

    server(rx);
}
