//! HTTP bridge primitives for interactive web sessions.

use axum::{
    body::Body,
    extract::Request,
    http::{Response, StatusCode},
};
use tokio::sync::{mpsc, oneshot};

pub type ResponseTx = oneshot::Sender<Response<Body>>;

pub async fn bridge_handler(
    req: Request,
    request_tx: mpsc::Sender<(super::HttpRequestFrame, ResponseTx)>,
) -> Response<Body> {
    let frame = match request_to_frame(req).await {
        Ok(f) => f,
        Err(status) => {
            return Response::builder()
                .status(status)
                .body(Body::empty())
                .unwrap();
        }
    };
    let (resp_tx, resp_rx) = oneshot::channel();
    if request_tx.send((frame, resp_tx)).await.is_err() {
        return Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(Body::from("server shutdown"))
            .unwrap();
    }
    match resp_rx.await {
        Ok(response) => response,
        Err(_) => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("handler dropped"))
            .unwrap(),
    }
}

async fn request_to_frame(req: Request) -> Result<super::HttpRequestFrame, StatusCode> {
    let (parts, body) = req.into_parts();
    let body_bytes = axum::body::to_bytes(body, 1_048_576)
        .await
        .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
    let body = body_bytes.to_vec();

    let method = parts.method.to_string();
    let path = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| "/".to_string());
    let http_version = format!(
        "1.{}",
        match parts.version {
            axum::http::Version::HTTP_09 => 0,
            axum::http::Version::HTTP_10 => 0,
            axum::http::Version::HTTP_11 => 1,
            axum::http::Version::HTTP_2 => 2,
            axum::http::Version::HTTP_3 => 3,
            _ => 1,
        }
    );

    let mut headers = Vec::with_capacity(parts.headers.len());
    for (name, value) in parts.headers.iter() {
        if let Ok(v) = value.to_str() {
            headers.push((name.as_str().to_string(), v.to_string()));
        }
    }

    let keep_alive = parts
        .headers
        .get(axum::http::header::CONNECTION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("keep-alive"))
        .unwrap_or(true);

    Ok(super::HttpRequestFrame {
        method,
        path,
        http_version,
        headers,
        body,
        keep_alive_requested: keep_alive,
    })
}

pub fn response_from_frame(frame: &super::HttpResponseFrame) -> Result<Response<Body>, String> {
    let mut response = Response::builder().status(frame.status_code);
    for (name, value) in &frame.headers {
        response = response.header(name.as_str(), value.as_str());
    }
    if !frame
        .headers
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case("content-length"))
    {
        response = response.header("Content-Length", frame.body.len().to_string());
    }
    if frame.should_close_connection {
        response = response.header("Connection", "close");
    }
    response
        .body(Body::from(frame.body.clone()))
        .map_err(|e| e.to_string())
}

pub fn response_from_vectored(
    head_bytes: &[u8],
    body_bytes: Vec<u8>,
) -> Result<Response<Body>, String> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut resp = httparse::Response::new(&mut headers);
    match resp
        .parse(head_bytes)
        .map_err(|e| format!("parse response head: {e}"))?
    {
        httparse::Status::Complete(_) => {}
        httparse::Status::Partial => return Err("incomplete response head".to_string()),
    }
    let status = resp.code.ok_or("missing status code")?;
    let headers_part: Vec<_> = resp
        .headers
        .iter()
        .map(|h| {
            (
                h.name.to_string(),
                std::str::from_utf8(h.value).unwrap_or("").to_string(),
            )
        })
        .collect();

    let mut builder = Response::builder().status(status);
    for (name, value) in &headers_part {
        builder = builder.header(name.as_str(), value.as_str());
    }
    if !headers_part
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case("content-length"))
    {
        builder = builder.header("Content-Length", body_bytes.len().to_string());
    }
    builder
        .body(Body::from(body_bytes))
        .map_err(|e| e.to_string())
}
