/// generic.rs
///
/// This example demonstrates how we can use traits to send values through a
/// channel without actually knowing the type of the value.
extern crate session_types_ng;

use session_types_ng::*;

fn srv<A>(x: A, c: Chan<mpsc::Channel, (), Send<mpsc::Value<A>, End>>) where A: std::marker::Send + 'static {
    c.send(mpsc::Value(x)).unwrap().close();
}

fn cli<A>(c: Chan<mpsc::Channel, (), Recv<mpsc::Value<A>, End>>) where A: std::marker::Send + std::fmt::Debug + 'static {
    let (c, mpsc::Value(x)) = c.recv().unwrap();
    println!("{:?}", x);
    c.close();
}

fn main() {
    let (c1, c2) = mpsc::session_channel();
    let t = std::thread::spawn(move || srv(42u8, c1));
    cli(c2);

    t.join().unwrap();
}
