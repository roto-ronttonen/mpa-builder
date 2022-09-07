use std::{io::Read, thread::spawn};

use rustc_serialize::base64::{Config, Newline, Standard, ToBase64};
use sha1::{Digest, Sha1};

/// Turns a Sec-WebSocket-Key into a Sec-WebSocket-Accept.
/// Feel free to copy-paste this function, but please use a better error handling.
fn convert_key(input: &str) -> String {
    let mut input = input.to_string().into_bytes();
    let mut bytes = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
        .to_string()
        .into_bytes();
    input.append(&mut bytes);

    let mut sha1 = Sha1::new();
    sha1.update(&input);

    sha1.finalize().to_base64(Config {
        char_set: Standard,
        pad: true,
        line_length: None,
        newline: Newline::LF,
    })
}

pub fn start() {
    let server = tiny_http::Server::http("0.0.0.0:4242").unwrap();
    let port = server.server_addr().port();

    println!("websocket server listening on port 4242");

    for request in server.incoming_requests() {
        // we are handling this websocket connection in a new task
        spawn(move || {
            // checking the "Upgrade" header to check that it is a websocket
            match request
                .headers()
                .iter()
                .find(|h| h.field.equiv(&"Upgrade"))
                .and_then(|hdr| {
                    if hdr.value == "websocket" {
                        Some(hdr)
                    } else {
                        None
                    }
                }) {
                _ => (),
            };

            // getting the value of Sec-WebSocket-Key
            let key = match request
                .headers()
                .iter()
                .find(|h| h.field.equiv(&"Sec-WebSocket-Key"))
                .map(|h| h.value.clone())
            {
                None => {
                    let response = tiny_http::Response::new_empty(tiny_http::StatusCode(400));
                    request.respond(response).expect("Responded");
                    return;
                }
                Some(k) => k,
            };

            // building the "101 Switching Protocols" response
            let response = tiny_http::Response::new_empty(tiny_http::StatusCode(101))
                .with_header("Upgrade: websocket".parse::<tiny_http::Header>().unwrap())
                .with_header("Connection: Upgrade".parse::<tiny_http::Header>().unwrap())
                .with_header(
                    "Sec-WebSocket-Protocol: ping"
                        .parse::<tiny_http::Header>()
                        .unwrap(),
                )
                .with_header(
                    format!("Sec-WebSocket-Accept: {}", convert_key(key.as_str()))
                        .parse::<tiny_http::Header>()
                        .unwrap(),
                );

            //
            let mut stream = request.upgrade("websocket", response);

            //
            loop {
                let mut out = Vec::new();
                match Read::by_ref(&mut stream).take(1).read_to_end(&mut out) {
                    Ok(n) if n >= 1 => {
                        // "Hello" frame
                        let data = [0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
                        stream.write(&data).ok();
                        stream.flush().ok();
                    }
                    Ok(_) => panic!("eof ; should never happen"),
                    Err(e) => {
                        println!("closing connection because: {}", e);
                        return;
                    }
                };
            }
        });
    }
}
