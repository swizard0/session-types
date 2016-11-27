// This is an implementation of the Sutherland-Hodgman (1974) reentrant polygon
// clipping algorithm. It takes a polygon represented as a number of vertices
// and cuts it according to the given planes.

// The implementation borrows heavily from Pucella-Tov (2008). See that paper
// for more explanation.
extern crate session_types_ng;
extern crate rand;

use std::thread::spawn;
use rand::{Rand, Rng};

use session_types_ng::*;

#[derive(Debug, Copy, Clone)]
struct Point(f64, f64, f64);

impl Rand for Point {
    fn rand<R: Rng>(rng: &mut R) -> Self {
        Point(rng.next_f64(), rng.next_f64(), rng.next_f64())
    }
}

#[derive(Debug, Copy, Clone)]
struct Plane(f64, f64, f64, f64);

impl Rand for Plane {
    fn rand<R: Rng>(rng: &mut R) -> Self {
        Plane(rng.next_f64(), rng.next_f64(), rng.next_f64(), rng.next_f64())
    }
}

fn above(Point(x, y, z): Point, Plane(a, b, c, d): Plane) -> bool {
    (a * x + b * y + c * z + d) / (a * a + b * b + c * c).sqrt() > 0.0
}

fn intersect(p1: Point, p2: Point, plane: Plane) -> Option<Point> {
    let Point(x1, y1, z1) = p1;
    let Point(x2, y2, z2) = p2;
    let Plane(a, b, c, d) = plane;

    if above(p1, plane) == above(p2, plane) {
        None
    } else {
        let t = (a * x1 + b * y1 + c * z1 + d) /
            (a * (x1 - x2) + b * (y1 - y2) + c * (z1 - z2));
        let x = x1 + (x2 - x1) * t;
        let y = y1 + (y2 - y1) * t;
        let z = z1 + (z2 - z1) * t;
        Some(Point(x, y, z))
    }
}

type SendList<A> = Rec<Choose<End, More<Choose<Send<mpsc::Value<A>, Var<Z>>, Nil>>>>;
type RecvList<A> = Rec<Offer<End, More<Offer<Recv<mpsc::Value<A>, Var<Z>>, Nil>>>>;

fn send_list<A>(chan: Chan<mpsc::Channel, (), SendList<A>>, xs: Vec<A>) where A: std::marker::Send + Copy + 'static
{
    let mut chan = chan.enter();
    for x in xs {
        chan = chan.second().unwrap().send(mpsc::Value(x)).unwrap().zero();
    }
    chan.first().unwrap().close();
}

fn recv_list<A>(chan: Chan<mpsc::Channel, (), RecvList<A>>) -> Vec<A> where A: std::marker::Send + 'static
{
    let mut vec = Vec::new();
    let mut chan = chan.enter();
    loop {
        let maybe_chan = chan
            .offer()
            .option(|chan_stop| {
                chan_stop.close();
                None
            })
            .option(|chan_value| {
                let (chan, mpsc::Value(x)) = chan_value.recv().unwrap();
                vec.push(x);
                Some(chan.zero())
            })
            .unwrap();

        if let Some(next_chan) = maybe_chan {
            chan = next_chan;
        } else {
            return vec;
        }
    }
}

fn clipper(plane: Plane,
           ic: Chan<mpsc::Channel, (), RecvList<Point>>,
           oc: Chan<mpsc::Channel, (), SendList<Point>>)
{
    let mut oc = oc.enter();
    let mut ic = ic.enter();
    let (pt0, mut pt);

    let maybe_values = ic
        .offer()
        .option(|chan_stop| {
            chan_stop.close();
            None
        })
        .option(|chan_value| {
            let (chan, mpsc::Value(ptz)) = chan_value.recv().unwrap();
            Some((ptz, chan.zero()))
        })
        .unwrap();

    if let Some((next_pt, next_ic)) = maybe_values {
        ic = next_ic;
        pt0 = next_pt;
        pt = next_pt;
    } else {
        oc.first().unwrap().close();
        return;
    }

    loop {
        if above(pt, plane) {
            oc = oc.second().unwrap().send(mpsc::Value(pt)).unwrap().zero();
        }

        let maybe_values = ic
            .offer()
            .option(|chan_stop| {
                chan_stop.close();
                None
            })
            .option(|chan_value| {
                let (ic, mpsc::Value(pt2)) = chan_value.recv().unwrap();
                Some((pt2, ic.zero()))
            })
            .unwrap();

        if let Some((pt2, next_ic)) = maybe_values {
            if let Some(pt) = intersect(pt, pt2, plane) {
                oc = oc.second().unwrap().send(mpsc::Value(pt)).unwrap().zero();
            }
            pt = pt2;
            ic = next_ic;
        } else {
            if let Some(pt) = intersect(pt, pt0, plane) {
                oc = oc.second().unwrap().send(mpsc::Value(pt)).unwrap().zero();
            }
            oc.first().unwrap().close();
            break;
        }
    }
}

fn clipmany(planes: Vec<Plane>, points: Vec<Point>) -> Vec<Point> {
    let (tx, rx) = mpsc::session_channel();
    spawn(move || send_list(tx, points));
    let mut rx = rx;

    for plane in planes {
        let (tx2, rx2) = mpsc::session_channel();
        spawn(move || clipper(plane, rx, tx2));
        rx = rx2;
    }
    recv_list(rx)
}

fn normalize_point(Point(a,b,c): Point) -> Point {
    Point(10.0 * (a - 0.5), 10.0 * (b - 0.5), 10.0 * (c - 0.5))
}

fn normalize_plane(Plane(a,b,c,d): Plane) -> Plane {
    Plane(10.0 * (a - 0.5), 10.0 * (b - 0.5), 10.0 * (c - 0.5), 10.0 * (d - 0.5))
}

fn bench(n: usize, m: usize) {
    let mut g = rand::thread_rng();
    let points = (0..n)
        .map(|_| rand::Rand::rand(&mut g))
        .map(normalize_point)
        .collect();
    let planes = (0..m)
        .map(|_| rand::Rand::rand(&mut g))
        .map(normalize_plane)
        .collect();

    let points = clipmany(planes, points);
    println!("bench: {} of {} points and {} planes", points.len(), n, m);
}

fn main() {
    bench(100, 5);
}
