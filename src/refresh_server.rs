use std::sync::{Arc, Mutex};

use tiny_http::{Response, Server};

pub fn start_refresh_server(token: Arc<Mutex<i32>>) {
    let server = Server::http("0.0.0.0:4242").unwrap();

    for request in server.incoming_requests() {
        let t = *token.lock().unwrap();
        let response = Response::from_string(t.to_string()).with_header(
            "Access-Control-Allow-Origin: *"
                .parse::<tiny_http::Header>()
                .unwrap(),
        );

        request.respond(response).unwrap();
    }
}
