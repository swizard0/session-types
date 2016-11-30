// This is an implementation of the extended arithmetic server from
// Vasconcelos-Gay-Ravara (2006) with some additional functionality

extern crate session_types_ng;

use std::marker;
use std::thread::spawn;
use session_types_ng::*;

// Offers: Add, Negate, Sqrt, Eval
type SrvQuit = End;
type SrvAdd  = Recv<i64, Recv<i64, Send<i64, Var<Z>>>>;
type SrvNeg  = Recv<i64, Send<i64, Var<Z>>>;
type SrvSqrt = Recv<f64, Choose<Send<f64, Var<Z>>, Choose<Var<Z>, Nil>>>;
type SrvEval = Recv<fn(i64) -> bool, Recv<i64, Send<bool, Var<Z>>>>;

type Srv =
    Offer<SrvQuit,
    Offer<SrvAdd,
    Offer<SrvNeg,
    Offer<SrvSqrt,
    Offer<SrvEval, Nil>>>>>;

fn server(chan: Chan<mpsc::Channel, (), Rec<Srv>>) {
    let mut chan = chan.enter();
    loop {
        let maybe_chan = chan
            .offer()
            .option(|chan_close| {
                chan_close.close();
                None
            })
            .option(|chan_add| {
                let (chan_add, n) = chan_add.recv().unwrap();
                let (chan_add, m) = chan_add.recv().unwrap();
                Some(chan_add.send(n + m).unwrap().zero())
            })
            .option(|chan_neg| {
                let (chan_neg, n) = chan_neg.recv().unwrap();
                Some(chan_neg.send(-n).unwrap().zero())
            })
            .option(|chan_sqrt| {
                let (chan_sqrt, x) = chan_sqrt.recv().unwrap();
                Some(if x >= 0.0 {
                    chan_sqrt.first().unwrap().send(x.sqrt()).unwrap().zero()
                } else {
                    chan_sqrt.second().unwrap().zero()
                })
            })
            .option(|chan_eval| {
                let (chan_eval, f) = chan_eval.recv().unwrap();
                let (chan_eval, n) = chan_eval.recv().unwrap();
                Some(chan_eval.send(f(n)).unwrap().zero())
            })
            .unwrap();

        if let Some(next_chan) = maybe_chan {
            chan = next_chan;
        } else {
            return;
        }
    }
}

// `add_client`, `neg_client` and `sqrt_client` are all pretty straightforward
// uses of session types, but they do showcase subtyping, recursion and how to
// work the types in general.

type AddCli<R> =
    Choose<End,
    Choose<Send<i64, Send<i64, Recv<i64, Var<Z>>>>, R>>;

fn add_client<R>(chan: Chan<mpsc::Channel, (), Rec<AddCli<R>>>) {
    let (chan, n) = chan
        .enter()
        .second().unwrap()
        .send(42).unwrap()
        .send(1).unwrap()
        .recv().unwrap();
    println!("add_client: {}", n);
    chan.zero().first().unwrap().close()
}

type NegCli<R, S> =
    Choose<End,
    Choose<R,
    Choose<Send<i64, Recv<i64, Var<Z>>>, S>>>;

fn neg_client<R, S>(chan: Chan<mpsc::Channel, (), Rec<NegCli<R, S>>>) {
    let (chan, n) = chan
        .enter()
        .third().unwrap()
        .send(42).unwrap()
        .recv().unwrap();
    println!("neg_client: {}", n);
    chan.zero().first().unwrap().close();
}

type SqrtCli<R, S, T> =
    Choose<End,
    Choose<R,
    Choose<S,
    Choose<Send<f64, Offer<Recv<f64, Var<Z>>, Offer<Var<Z>, Nil>>>, T>>>>;

fn sqrt_client<R, S, T>(chan: Chan<mpsc::Channel, (), Rec<SqrtCli<R, S, T>>>) {
    let () = chan
        .enter()
        .fourth().unwrap()
        .send(42.0).unwrap()
        .offer()
        .option(|chan_ok| {
            let (chan, n) = chan_ok.recv().unwrap();
            println!("sqrt_client: {} OK", n);
            chan.zero().first().unwrap().close();
        })
        .option(|chan_fail| {
            println!("sqrt_client: couldn't take square root!");
            chan_fail.zero().first().unwrap().close();
        })
        .unwrap();
}

// `fn_client` sends a function over the channel

type PrimeCli<R, S, T> =
    Choose<End,
    Choose<R,
    Choose<S,
    Choose<T,
    Choose<Send<fn(i64) -> bool, Send<i64, Recv<bool, Var<Z>>>>, Nil>>>>>;

fn fn_client<R, S, T>(chan: Chan<mpsc::Channel, (), Rec<PrimeCli<R, S, T>>>) {
    fn even(n: i64) -> bool {
        n % 2 == 0
    }

    let (chan, b) = chan
        .enter()
        .fifth().unwrap()
        .send(even).unwrap()
        .send(42).unwrap()
        .recv().unwrap();
    println!("fn_client: {}", b);
    chan.zero().first().unwrap().close();
}

// `ask_neg` and `get_neg` use delegation, that is, sending a channel over
// another channel.

// `ask_neg` selects the negation operation and sends an integer, whereafter it
// sends the whole channel to `get_neg`. `get_neg` then receives the negated
// integer and prints it.

type AskNeg<R, S> =
    Choose<End,
    Choose<R,
    Choose<Send<i64, Recv<i64, Var<Z>>>, S>>>;

type DelegChanSend<R, S> =
    Send<Chan<mpsc::Channel, (AskNeg<R, S>, ()), Recv<i64, Var<Z>>>, End>;

fn ask_neg<R, S>(c1: Chan<mpsc::Channel, (), Rec<AskNeg<R, S>>>,
                 c2: Chan<mpsc::Channel, (), DelegChanSend<R, S>>)
    where R: marker::Send + 'static, S: marker::Send + 'static
{
    let c1 = c1.enter().third().unwrap().send(42).unwrap();
    c2.send(c1).unwrap().close();
}

type DelegChanRecv<R, S> =
    Recv<Chan<mpsc::Channel, (AskNeg<R, S>, ()), Recv<i64, Var<Z>>>, End>;

fn get_neg<R, S>(c1: Chan<mpsc::Channel, (), DelegChanRecv<R, S>>)
    where R: marker::Send + 'static, S: marker::Send + 'static
{
    let (c1, c2) = c1.recv().unwrap();
    let (c2, n) = c2.recv().unwrap();
    println!("get_neg: {}", n);
    c2.zero().first().unwrap().close();
    c1.close();
}

fn main() {
    mpsc::connect(server, add_client);
    mpsc::connect(server, neg_client);
    mpsc::connect(server, sqrt_client);
    mpsc::connect(server, fn_client);

    let (c1, c1_) = mpsc::session_channel();
    let (c2, c2_) = mpsc::session_channel();

    let t1 = spawn(move || server(c1));
    let t2 = spawn(move || ask_neg(c1_, c2));
    let t3 = spawn(move || get_neg(c2_));

    let _ = t1.join();
    let _ = t2.join();
    let _ = t3.join();
}
