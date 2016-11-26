// This is an implementation of the extended arithmetic server from
// Vasconcelos-Gay-Ravara (2006) with some additional functionality

#[macro_use]
extern crate session_types_ng;

use std::marker;
use std::thread::spawn;
use session_types_ng::*;


// Offers: Add, Negate, Sqrt, Eval
type SrvQuit = End;
type SrvAdd  = Recv<mpsc::Value<i64>, Recv<mpsc::Value<i64>, Send<mpsc::Value<i64>, Var<Z>>>>;
type SrvNeg  = Recv<mpsc::Value<i64>, Send<mpsc::Value<i64>, Var<Z>>>;
type SrvSqrt = Recv<mpsc::Value<f64>, Choose<Send<mpsc::Value<f64>, Var<Z>>, More<Choose<Var<Z>, Nil>>>>;
type SrvEval = Recv<mpsc::Value<fn(i64) -> bool>, Recv<mpsc::Value<i64>, Send<mpsc::Value<bool>, Var<Z>>>>;

type Srv =
    Offer<SrvQuit, More<
    Offer<SrvAdd, More<
    Offer<SrvNeg, More<
    Offer<SrvSqrt, More<
    Offer<SrvEval, Nil>>>>>>>>>;

fn server(chan: Chan<mpsc::Channel, (), Rec<Srv>>) {
    let mut chan = chan.enter();
    loop {
        enum Action {
            Stop,
            Next(Chan<mpsc::Channel, (Srv, ()), Srv>),
        }

        let action = chan
            .offer()
            .option(|chan_close| {
                chan_close.close();
                Action::Stop
            })
            .option(|chan_add| {
                let (chan_add, mpsc::Value(n)) = chan_add.recv().unwrap();
                let (chan_add, mpsc::Value(m)) = chan_add.recv().unwrap();
                Action::Next(chan_add.send(mpsc::Value(n + m)).unwrap().zero())
            })
            .option(|chan_neg| {
                let (chan_neg, mpsc::Value(n)) = chan_neg.recv().unwrap();
                Action::Next(chan_neg.send(mpsc::Value(-n)).unwrap().zero())
            })
            .option(|chan_sqrt| {
                let (chan_sqrt, mpsc::Value(x)) = chan_sqrt.recv().unwrap();
                Action::Next(if x >= 0.0 {
                    chan_sqrt.head().unwrap().send(mpsc::Value(x.sqrt())).unwrap().zero()
                } else {
                    chan_sqrt.tail().unwrap().head().unwrap().zero()
                })
            })
            .option(|chan_eval| {
                let (chan_eval, mpsc::Value(f)) = chan_eval.recv().unwrap();
                let (chan_eval, mpsc::Value(n)) = chan_eval.recv().unwrap();
                Action::Next(chan_eval.send(mpsc::Value(f(n))).unwrap().zero())
            })
            .unwrap();

        match action {
            Action::Stop =>
                return,
            Action::Next(next_chan) =>
                chan = next_chan,
        };
    }
}

// `add_client`, `neg_client` and `sqrt_client` are all pretty straightforward
// uses of session types, but they do showcase subtyping, recursion and how to
// work the types in general.

type AddCli<R> =
    Choose<End, More<
    Choose<Send<mpsc::Value<i64>, Send<mpsc::Value<i64>, Recv<mpsc::Value<i64>, Var<Z>>>>, R>>>;

fn add_client<R>(chan: Chan<mpsc::Channel, (), Rec<AddCli<R>>>) {
    let (chan, mpsc::Value(n)) = chan
        .enter()
        .tail().unwrap()
        .head().unwrap()
        .send(mpsc::Value(42)).unwrap()
        .send(mpsc::Value(1)).unwrap()
        .recv().unwrap();
    println!("add_client: {}", n);
    chan.zero().head().unwrap().close()
}

type NegCli<R, S> =
    Choose<End, More<
    Choose<R, More<
    Choose<Send<mpsc::Value<i64>, Recv<mpsc::Value<i64>, Var<Z>>>, S>>>>>;

fn neg_client<R, S>(chan: Chan<mpsc::Channel, (), Rec<NegCli<R, S>>>) {
    let (chan, mpsc::Value(n)) = chan
        .enter()
        .skip2().unwrap()
        .head().unwrap()
        .send(mpsc::Value(42)).unwrap()
        .recv().unwrap();
    println!("neg_client: {}", n);
    chan.zero().head().unwrap().close();
}

type SqrtCli<R, S, T> =
    Choose<End, More<
    Choose<R, More<
    Choose<S, More<
    Choose<Send<mpsc::Value<f64>, Offer<Recv<mpsc::Value<f64>, Var<Z>>, More<Offer<Var<Z>, Nil>>>>, T>>>>>>>;

fn sqrt_client<R, S, T>(chan: Chan<mpsc::Channel, (), Rec<SqrtCli<R, S, T>>>) {
    let () = chan
        .enter()
        .skip3().unwrap()
        .head().unwrap()
        .send(mpsc::Value(42.0)).unwrap()
        .offer()
        .option(|chan_ok| {
            let (chan, mpsc::Value(n)) = chan_ok.recv().unwrap();
            println!("sqrt_client: {} OK", n);
            chan.zero().head().unwrap().close();
        })
        .option(|chan_fail| {
            println!("sqrt_client: couldn't take square root!");
            chan_fail.zero().head().unwrap().close();
        })
        .unwrap();
}

// `fn_client` sends a function over the channel

type PrimeCli<R, S, T> =
    Choose<End, More<
    Choose<R, More<
    Choose<S, More<
    Choose<T, More<
    Choose<Send<mpsc::Value<fn(i64) -> bool>, Send<mpsc::Value<i64>, Recv<mpsc::Value<bool>, Var<Z>>>>, Nil>>>>>>>>>;

fn fn_client<R, S, T>(chan: Chan<mpsc::Channel, (), Rec<PrimeCli<R, S, T>>>) {
    fn even(n: i64) -> bool {
        n % 2 == 0
    }

    let (chan, mpsc::Value(b)) = chan
        .enter()
        .skip4().unwrap()
        .head().unwrap()
        .send(mpsc::Value(even)).unwrap()
        .send(mpsc::Value(42)).unwrap()
        .recv().unwrap();
    println!("fn_client: {}", b);
    chan.zero().head().unwrap().close();
}


// // `ask_neg` and `get_neg` use delegation, that is, sending a channel over
// // another channel.

// // `ask_neg` selects the negation operation and sends an integer, whereafter it
// // sends the whole channel to `get_neg`. `get_neg` then receives the negated
// // integer and prints it.

// type AskNeg<R, S> =
//     Choose<Eps,
//     Choose<R,
//     Choose<Send<i64, Recv<i64, Var<Z>>>,
//     S>>>;


// fn ask_neg<R, S>(c1: Chan<mpsc::Channel, (), Rec<AskNeg<R, S>>>,
//                  c2: Chan<mpsc::Channel, (), Send<Chan<mpsc::Channel, (AskNeg<R, S>, ()), Recv<i64, Var<Z>>>, Eps>>) where
//     R: marker::Send + 'static, S: marker::Send + 'static
// {
//     let c1 = c1.enter().sel2().sel2().sel1().send(42);
//     c2.send(c1).close();
// }

// fn get_neg<R, S>(c1: Chan<mpsc::Channel, (), Recv<Chan<mpsc::Channel, (AskNeg<R, S>, ()), Recv<i64, Var<Z>>>, Eps>>) where
//     R: marker::Send + 'static, S: marker::Send + 'static
// {
//     let (c1, c2) = c1.recv();
//     let (c2, n) = c2.recv();
//     println!("{}", n);
//     c2.zero().sel1().close();
//     c1.close();
// }

fn main() {
    mpsc::connect(server, add_client);
    mpsc::connect(server, neg_client);
    mpsc::connect(server, sqrt_client);
    mpsc::connect(server, fn_client);

    // let (c1, c1_) = mpsc::session_channel();
    // let (c2, c2_) = mpsc::session_channel();

    // let t1 = spawn(move || server(c1));
    // let t2 = spawn(move || ask_neg(c1_, c2));
    // let t3 = spawn(move || get_neg(c2_));

    // let _ = t1.join();
    // let _ = t2.join();
    // let _ = t3.join();
}
