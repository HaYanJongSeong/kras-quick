use kras_quick::cdp_surface::CdpSurface;
use kras_quick::property_watcher_runtime::CdpTargetPage;
use serde_json::{json, Value};
use std::net::TcpListener;
use std::time::Duration;

fn respond(reply: Value) -> Result<Value, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline, "client never connected");
                    std::thread::yield_now();
                }
                Err(error) => panic!("fixture accept failed: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut socket = tungstenite::accept(stream).unwrap();
        let command: Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(command["id"], 1);
        assert_eq!(command["method"], "Runtime.evaluate");
        socket.send(tungstenite::Message::Text(reply.to_string().into())).unwrap();
        // 응답을 받은 클라이언트가 연결을 닫을 때까지 서버를 유지한다.
        let _ = socket.read();
    });
    let page = CdpTargetPage { id: "fixture".into(), url: "https://fixture.invalid/".into(), ws_url: format!("ws://{address}") };
    let surface = CdpSurface::from_page(&format!("http://{address}"), &page);
    let result = surface.send_cdp("Runtime.evaluate", json!({"expression":"void 0","returnByValue":true}));
    server.join().unwrap();
    result
}

#[test]
fn nested_javascript_exception_is_not_a_successful_empty_result() {
    let result = respond(json!({"id":1,"result":{
        "result":{"type":"undefined"},
        "exceptionDetails":{"text":"Uncaught","exception":{"description":"TypeError: DOM_FAILURE"}}
    }}));
    assert!(result.is_err(), "CDP result.exceptionDetails must propagate as an error, not an empty result");
    assert!(result.unwrap_err().to_string().contains("DOM_FAILURE"));
}

#[test]
fn nested_exception_without_description_keeps_protocol_text() {
    let error = respond(json!({"id":1,"result":{"exceptionDetails":{"text":"SCRIPT_FAILURE"}}})).unwrap_err();
    assert!(error.to_string().contains("SCRIPT_FAILURE"));
}

#[test]
fn successful_remote_value_is_unchanged() {
    let payload = json!({"result":{"type":"object","value":{"count":7,"code":"00123"}}});
    assert_eq!(respond(json!({"id":1,"result":payload})).unwrap(), payload);
}

#[test]
fn protocol_error_still_propagates() {
    let error = respond(json!({"id":1,"error":{"code":-32601,"message":"METHOD_NOT_FOUND"}})).unwrap_err();
    assert!(error.to_string().contains("-32601"));
}

#[test]
fn top_level_exception_still_propagates() {
    // Given a legacy peer placing exceptionDetails outside result.
    let reply = json!({"id":1,"exceptionDetails":{"exception":{"value":"LEGACY_FAILURE"}}});
    // When the matching command response arrives.
    let error = respond(reply).unwrap_err();
    // Then the remote exception remains an error.
    assert!(error.to_string().contains("LEGACY_FAILURE"));
}
