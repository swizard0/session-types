extern crate session_types_ng;
use session_types_ng::*;

fn server(c: Chan<(), Eps>) {
    c.close()
}

fn client(c: Chan<(), Eps>) {
    c.close()
}

fn main() {
    connect(server, client);
}
