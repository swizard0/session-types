/// This is an implementation of an echo server.
/// One process reads input and sends it to the other process, which outputs it.
extern crate session_types_ng;

use session_types_ng::*;

type Srv = Offer<End, Offer<Recv<mpsc::Value<String>, Var<Z>>, Nil>>;

fn srv(chan: Chan<mpsc::Channel, (), Rec<Srv>>) {
    let mut chan = chan.enter();
    loop {
        let maybe_chan = chan
            .offer()
            .option(|chan_close| {
                println!("Closing server.");
                chan_close.close();
                None
            })
            .option(|chan_recv| {
                let (chan, mpsc::Value(s)) = chan_recv.recv().unwrap();
                println!("Received: {}", s);
                Some(chan.zero())
            })
            .unwrap();
        if let Some(next_chan) = maybe_chan {
            chan = next_chan;
        } else {
            break;
        }
    }
}

type Cli = <Srv as HasDual>::Dual;

fn cli(chan: Chan<mpsc::Channel, (), Rec<Cli>>) {
    let stdin = std::io::stdin();
    let mut count = 0usize;

    let mut chan = chan.enter();
    let mut buf = "".to_string();
    loop {
        stdin.read_line(&mut buf).ok().unwrap();
        if !buf.is_empty() {
            buf.pop();
        }
        match &buf[..] {
            "q" => {
                chan
                    .second().unwrap()
                    .send(mpsc::Value(format!("{} lines sent", count))).unwrap()
                    .zero()
                    .first().unwrap()
                    .close();
                println!("Client quitting");
                break;
            }
            _ => {
                chan = chan
                    .second().unwrap()
                    .send(mpsc::Value(buf.clone())).unwrap()
                    .zero();
                buf.clear();
                count += 1;
            }
        }
    }
}

fn main() {
    mpsc::connect(srv, cli);
}
