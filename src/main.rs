use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, StatusCode, Server};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use hyper::header::{CONTENT_TYPE, CACHE_CONTROL};


#[derive(Debug, Clone)]
struct AppState {
    clients: Arc<Mutex<Vec<broadcast::Sender<String>>>>,
}

async fn handle_request(req: Request<Body>, state: AppState) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(response_build(
            "The valid endpoints are /init /create_order /create_orders /update_order /orders /delete_order /sse",
        )),

        (&Method::GET, "/sse") => {
            // Create a new broadcast channel for this client
            let (tx, mut rx) = broadcast::channel(16);


            // Add the new client to the list
            state.clients.lock().unwrap().push(tx);

            // Stream SSE events to the client
            let body_stream = async_stream::stream! {
                while let Ok(message) = rx.recv().await {
                    yield Ok::<_, hyper::Error>(format!("data: {}\n\n", message));
                }
            };

            Ok(Response::builder()
                .header(CONTENT_TYPE, "text/event-stream")
                .header(CACHE_CONTROL, "no-cache")
                .body(Body::wrap_stream(body_stream))
                .unwrap())
        },

        (&Method::POST, "/echo") => Ok(Response::new(req.into_body())),

        _ => {
            let mut not_found = Response::default();
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

fn response_build(body: &str) -> Response<Body> {
    Response::builder()
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        .header("Access-Control-Allow-Headers", "api,Keep-Alive,User-Agent,Content-Type")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));

    // Create a list of clients
    let clients = Arc::new(Mutex::new(Vec::new()));
    let state = AppState { clients };

    // Spawn a periodic task to send messages to all connected clients
    let clients_clone = state.clients.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await; // Wait for the next tick
            let message = format!("Current time: {:?}", chrono::Utc::now());
            let clients = clients_clone.lock().unwrap();
            println!("Number of active clients: {}", clients.len());
            for client_tx in clients.iter() {
                if let Err(e) = client_tx.send(message.clone()) {
                    eprintln!("Failed to send message: {}", e);

                }
                            // Remove closed channels
            }
        }
    });

    let make_svc = make_service_fn(move |_| {
        let state = state.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let state = state.clone();
                handle_request(req, state)
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }

    Ok(())
}
