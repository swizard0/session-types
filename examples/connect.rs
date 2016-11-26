extern crate session_types_ng;

use session_types_ng::*;

fn server(c: Chan<mpsc::Channel, (), End>) {
    c.close()
}

fn client(c: Chan<mpsc::Channel, (), End>) {
    c.close()
}

fn main() {
    mpsc::connect(server, client);
}
