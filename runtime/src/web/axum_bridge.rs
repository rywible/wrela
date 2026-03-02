//! Protocol and WebSocket transport primitives for game sessions.

use axum::{
    body::Body,
    extract::Request,
    http::{Response, StatusCode},
};
use base64::Engine as _;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};

pub(crate) const PROTOCOL_V5_VERSION: u16 = 5;
pub(crate) const PROTOCOL_V5_SUB_VERSION: u16 = 0;
pub(crate) const MESSAGE_TYPE_HELLO_V5: u16 = 1;
#[allow(dead_code)]
pub(crate) const MESSAGE_TYPE_AUTH_OK_V5: u16 = 2;
pub(crate) const MESSAGE_TYPE_INPUT_BATCH_V5: u16 = 3;
pub(crate) const MESSAGE_TYPE_SNAPSHOT_V5: u16 = 4;
pub(crate) const MESSAGE_TYPE_DELTA_V5: u16 = 5;
pub(crate) const MESSAGE_TYPE_CORRECTION_V5: u16 = 6;
#[allow(dead_code)]
pub(crate) const MESSAGE_TYPE_RESUME_V5: u16 = 7;
pub(crate) const MESSAGE_TYPE_PING_V5: u16 = 8;
pub(crate) const MESSAGE_TYPE_ERROR_V5: u16 = 9;

pub(crate) const PROTOCOL_V5_HEADER_LEN: usize = 62;

const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
const MAX_WS_FRAME_BYTES: usize = 4 * 1024 * 1024;

pub type ResponseTx = oneshot::Sender<Response<Body>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProtocolEnvelope {
    pub version: u16,
    pub sub_version: u16,
    pub session_id: u64,
    pub partition_id: u64,
    pub actor_id: u64,
    pub message_type: u16,
    pub tick: u64,
    pub seq: u64,
    pub ack: u64,
    pub payload_len: u32,
    pub crc32: u32,
    pub payload: Vec<u8>,
}

impl ProtocolEnvelope {
    pub(crate) fn new(
        version: u16,
        sub_version: u16,
        session_id: u64,
        partition_id: u64,
        actor_id: u64,
        message_type: u16,
        tick: u64,
        seq: u64,
        ack: u64,
        payload: Vec<u8>,
    ) -> Self {
        let payload_len = payload.len().min(u32::MAX as usize) as u32;
        let crc32 = crc32fast::hash(&payload);
        Self {
            version,
            sub_version,
            session_id,
            partition_id,
            actor_id,
            message_type,
            tick,
            seq,
            ack,
            payload_len,
            crc32,
            payload,
        }
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let payload_len = self.payload.len().min(u32::MAX as usize) as u32;
        let crc32 = crc32fast::hash(&self.payload);
        let mut out = Vec::with_capacity(PROTOCOL_V5_HEADER_LEN + payload_len as usize);
        out.extend_from_slice(&self.version.to_be_bytes());
        out.extend_from_slice(&self.sub_version.to_be_bytes());
        out.extend_from_slice(&self.session_id.to_be_bytes());
        out.extend_from_slice(&self.partition_id.to_be_bytes());
        out.extend_from_slice(&self.actor_id.to_be_bytes());
        out.extend_from_slice(&self.message_type.to_be_bytes());
        out.extend_from_slice(&self.tick.to_be_bytes());
        out.extend_from_slice(&self.seq.to_be_bytes());
        out.extend_from_slice(&self.ack.to_be_bytes());
        out.extend_from_slice(&payload_len.to_be_bytes());
        out.extend_from_slice(&crc32.to_be_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < PROTOCOL_V5_HEADER_LEN {
            return Err("protocol frame shorter than protocol-v5 header".to_string());
        }

        let version = u16::from_be_bytes([bytes[0], bytes[1]]);
        let sub_version = u16::from_be_bytes([bytes[2], bytes[3]]);
        if version != PROTOCOL_V5_VERSION || sub_version != PROTOCOL_V5_SUB_VERSION {
            return Err(format!(
                "unsupported protocol version={} sub_version={}; expected protocol={} sub_version={}",
                version, sub_version, PROTOCOL_V5_VERSION, PROTOCOL_V5_SUB_VERSION
            ));
        }
        let session_id = u64::from_be_bytes([
            bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11],
        ]);
        let partition_id = u64::from_be_bytes([
            bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19],
        ]);
        let actor_id = u64::from_be_bytes([
            bytes[20], bytes[21], bytes[22], bytes[23], bytes[24], bytes[25], bytes[26], bytes[27],
        ]);
        let message_type = u16::from_be_bytes([bytes[28], bytes[29]]);
        let tick = u64::from_be_bytes([
            bytes[30], bytes[31], bytes[32], bytes[33], bytes[34], bytes[35], bytes[36], bytes[37],
        ]);
        let seq = u64::from_be_bytes([
            bytes[38], bytes[39], bytes[40], bytes[41], bytes[42], bytes[43], bytes[44], bytes[45],
        ]);
        let ack = u64::from_be_bytes([
            bytes[46], bytes[47], bytes[48], bytes[49], bytes[50], bytes[51], bytes[52], bytes[53],
        ]);
        let payload_len = u32::from_be_bytes([bytes[54], bytes[55], bytes[56], bytes[57]]);
        let crc32 = u32::from_be_bytes([bytes[58], bytes[59], bytes[60], bytes[61]]);

        let expected_total = PROTOCOL_V5_HEADER_LEN + payload_len as usize;
        if bytes.len() != expected_total {
            return Err(format!(
                "protocol payload_len mismatch: header={payload_len} actual={}",
                bytes.len().saturating_sub(PROTOCOL_V5_HEADER_LEN)
            ));
        }

        let payload = bytes[PROTOCOL_V5_HEADER_LEN..].to_vec();
        let actual_crc32 = crc32fast::hash(&payload);
        if actual_crc32 != crc32 {
            return Err(format!(
                "crc_mismatch expected={crc32} actual={actual_crc32}"
            ));
        }

        Ok(Self {
            version,
            sub_version,
            session_id,
            partition_id,
            actor_id,
            message_type,
            tick,
            seq,
            ack,
            payload_len,
            crc32,
            payload,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WebSocketFrame {
    Binary(Vec<u8>),
    Text(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OutboundFrame {
    Binary(Vec<u8>),
    Pong(Vec<u8>),
    Close(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HttpHandshake {
    pub path: String,
    pub websocket_key: Option<String>,
    pub is_websocket_upgrade: bool,
}

pub(crate) async fn read_http_handshake(stream: &mut TcpStream) -> Result<HttpHandshake, String> {
    let mut buffer = Vec::with_capacity(1024);
    let mut scratch = [0u8; 1024];

    while !buffer.windows(4).any(|window| window == b"\r\n\r\n") {
        let read_len = stream
            .read(&mut scratch)
            .await
            .map_err(|error| format!("failed reading HTTP handshake: {error}"))?;
        if read_len == 0 {
            return Err("connection closed before HTTP handshake completed".to_string());
        }
        buffer.extend_from_slice(&scratch[..read_len]);
        if buffer.len() > MAX_HTTP_HEADER_BYTES {
            return Err("HTTP handshake exceeds maximum header size".to_string());
        }
    }

    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut headers);
    match request
        .parse(buffer.as_slice())
        .map_err(|error| format!("invalid HTTP handshake: {error}"))?
    {
        httparse::Status::Complete(_) => {}
        httparse::Status::Partial => {
            return Err("HTTP handshake parse returned partial request".to_string());
        }
    }

    let path = request.path.unwrap_or("/").to_string();
    let mut upgrade_value = String::new();
    let mut connection_value = String::new();
    let mut websocket_key = None;

    for header in request.headers.iter() {
        let header_name = header.name.to_ascii_lowercase();
        let header_value = String::from_utf8_lossy(header.value).trim().to_string();
        if header_name == "upgrade" {
            upgrade_value = header_value;
        } else if header_name == "connection" {
            connection_value = header_value;
        } else if header_name == "sec-websocket-key" {
            websocket_key = Some(header_value);
        }
    }

    let is_websocket_upgrade = upgrade_value.eq_ignore_ascii_case("websocket")
        && connection_value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("upgrade"));

    Ok(HttpHandshake {
        path,
        websocket_key,
        is_websocket_upgrade,
    })
}

pub(crate) async fn write_bootstrap_response(
    stream: &mut TcpStream,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    let response_head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    );
    stream
        .write_all(response_head.as_bytes())
        .await
        .map_err(|error| format!("failed writing bootstrap response head: {error}"))?;
    stream
        .write_all(body)
        .await
        .map_err(|error| format!("failed writing bootstrap response body: {error}"))?;
    stream
        .flush()
        .await
        .map_err(|error| format!("failed flushing bootstrap response: {error}"))
}

pub(crate) async fn write_websocket_upgrade_response(
    stream: &mut TcpStream,
    websocket_key: &str,
) -> Result<(), String> {
    let accept_value = websocket_accept_value(websocket_key.trim());
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept_value}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|error| format!("failed writing websocket upgrade response: {error}"))?;
    stream
        .flush()
        .await
        .map_err(|error| format!("failed flushing websocket upgrade response: {error}"))
}

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

pub(crate) async fn read_websocket_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Option<WebSocketFrame>, String> {
    let mut first_two = [0u8; 2];
    match reader.read_exact(&mut first_two).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => {
            return Err(format!("websocket read header failed: {error}"));
        }
    }

    let fin = (first_two[0] & 0x80) != 0;
    let opcode = first_two[0] & 0x0f;
    let masked = (first_two[1] & 0x80) != 0;

    let mut payload_len = (first_two[1] & 0x7f) as u64;
    if payload_len == 126 {
        let mut extended = [0u8; 2];
        reader
            .read_exact(&mut extended)
            .await
            .map_err(|error| format!("websocket read extended length (u16) failed: {error}"))?;
        payload_len = u16::from_be_bytes(extended) as u64;
    } else if payload_len == 127 {
        let mut extended = [0u8; 8];
        reader
            .read_exact(&mut extended)
            .await
            .map_err(|error| format!("websocket read extended length (u64) failed: {error}"))?;
        payload_len = u64::from_be_bytes(extended);
    }

    if payload_len > MAX_WS_FRAME_BYTES as u64 {
        return Err(format!(
            "websocket payload too large: {payload_len} > {MAX_WS_FRAME_BYTES}"
        ));
    }

    let mut mask_key = [0u8; 4];
    if masked {
        reader
            .read_exact(&mut mask_key)
            .await
            .map_err(|error| format!("websocket read mask key failed: {error}"))?;
    }

    let mut payload = vec![0u8; payload_len as usize];
    if payload_len > 0 {
        reader
            .read_exact(payload.as_mut_slice())
            .await
            .map_err(|error| format!("websocket read payload failed: {error}"))?;
    }

    if masked {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask_key[index % 4];
        }
    }

    if !fin {
        return Err("fragmented websocket frames are not supported".to_string());
    }

    let frame = match opcode {
        0x1 => WebSocketFrame::Text(payload),
        0x2 => WebSocketFrame::Binary(payload),
        0x8 => WebSocketFrame::Close(payload),
        0x9 => WebSocketFrame::Ping(payload),
        0xA => WebSocketFrame::Pong(payload),
        _ => return Err(format!("unsupported websocket opcode={opcode}")),
    };

    Ok(Some(frame))
}

pub(crate) async fn write_outbound_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: &OutboundFrame,
) -> Result<usize, String> {
    let (opcode, payload) = match frame {
        OutboundFrame::Binary(payload) => (0x2, payload.as_slice()),
        OutboundFrame::Pong(payload) => (0xA, payload.as_slice()),
        OutboundFrame::Close(payload) => (0x8, payload.as_slice()),
    };

    let frame_bytes = encode_frame_bytes(opcode, payload, false, [0u8; 4]);
    writer
        .write_all(frame_bytes.as_slice())
        .await
        .map_err(|error| format!("websocket write frame failed: {error}"))?;
    writer
        .flush()
        .await
        .map_err(|error| format!("websocket flush frame failed: {error}"))?;
    Ok(frame_bytes.len())
}

#[cfg(test)]
pub(crate) fn encode_client_binary_frame(payload: &[u8], mask_key: [u8; 4]) -> Vec<u8> {
    encode_frame_bytes(0x2, payload, true, mask_key)
}

fn encode_frame_bytes(opcode: u8, payload: &[u8], masked: bool, mask_key: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + payload.len() + 16);
    out.push(0x80 | (opcode & 0x0f));

    let mask_bit = if masked { 0x80 } else { 0x00 };
    if payload.len() <= 125 {
        out.push(mask_bit | payload.len() as u8);
    } else if payload.len() <= u16::MAX as usize {
        out.push(mask_bit | 126);
        out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        out.push(mask_bit | 127);
        out.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    }

    if masked {
        out.extend_from_slice(&mask_key);
        let mut masked_payload = payload.to_vec();
        for (index, byte) in masked_payload.iter_mut().enumerate() {
            *byte ^= mask_key[index % 4];
        }
        out.extend_from_slice(masked_payload.as_slice());
    } else {
        out.extend_from_slice(payload);
    }

    out
}

fn websocket_accept_value(websocket_key: &str) -> String {
    let mut input = Vec::with_capacity(websocket_key.len() + WS_GUID.len());
    input.extend_from_slice(websocket_key.as_bytes());
    input.extend_from_slice(WS_GUID.as_bytes());
    let digest = sha1_digest(input.as_slice());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

fn sha1_digest(message: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x6745_2301;
    let mut h1: u32 = 0xEFCD_AB89;
    let mut h2: u32 = 0x98BA_DCFE;
    let mut h3: u32 = 0x1032_5476;
    let mut h4: u32 = 0xC3D2_E1F0;

    let bit_len = (message.len() as u64) * 8;
    let mut data = message.to_vec();
    data.push(0x80);
    while (data.len() % 64) != 56 {
        data.push(0x00);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in data.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (index, word) in w.iter_mut().take(16).enumerate() {
            let start = index * 4;
            *word = u32::from_be_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }
        for index in 16..80 {
            w[index] = (w[index - 3] ^ w[index - 8] ^ w[index - 14] ^ w[index - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for (index, item) in w.iter().enumerate() {
            let (f, k) = if index < 20 {
                ((b & c) | ((!b) & d), 0x5A82_7999)
            } else if index < 40 {
                (b ^ c ^ d, 0x6ED9_EBA1)
            } else if index < 60 {
                ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC)
            } else {
                (b ^ c ^ d, 0xCA62_C1D6)
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*item);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_envelope_round_trip() {
        let source = ProtocolEnvelope::new(
            PROTOCOL_V5_VERSION,
            PROTOCOL_V5_SUB_VERSION,
            42,
            7,
            99,
            MESSAGE_TYPE_HELLO_V5,
            100,
            1,
            0,
            b"hello".to_vec(),
        );
        let encoded = source.encode();
        let decoded = ProtocolEnvelope::decode(encoded.as_slice()).expect("decode");
        assert_eq!(decoded.version, PROTOCOL_V5_VERSION);
        assert_eq!(decoded.sub_version, PROTOCOL_V5_SUB_VERSION);
        assert_eq!(decoded.session_id, 42);
        assert_eq!(decoded.partition_id, 7);
        assert_eq!(decoded.actor_id, 99);
        assert_eq!(decoded.message_type, MESSAGE_TYPE_HELLO_V5);
        assert_eq!(decoded.payload, b"hello");
    }

    #[test]
    fn protocol_envelope_rejects_crc_mismatch() {
        let source = ProtocolEnvelope::new(
            PROTOCOL_V5_VERSION,
            PROTOCOL_V5_SUB_VERSION,
            42,
            7,
            99,
            MESSAGE_TYPE_HELLO_V5,
            100,
            1,
            0,
            b"hello".to_vec(),
        );
        let mut encoded = source.encode();
        let last = encoded.len() - 1;
        encoded[last] ^= 0x7f;
        let decoded = ProtocolEnvelope::decode(encoded.as_slice());
        assert!(decoded.is_err());
        assert!(decoded.err().unwrap_or_default().contains("crc_mismatch"));
    }

    #[test]
    fn websocket_accept_key_matches_rfc_example() {
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let accept = websocket_accept_value(key);
        assert_eq!(accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn rejects_protocol_v4_after_cutover() {
        let source = ProtocolEnvelope::new(
            4,
            0,
            42,
            7,
            99,
            MESSAGE_TYPE_HELLO_V5,
            100,
            1,
            0,
            b"hello".to_vec(),
        );
        let encoded = source.encode();
        let err = ProtocolEnvelope::decode(encoded.as_slice()).expect_err("protocol-v4 must fail");
        assert!(err.contains("unsupported protocol version=4"));
    }
}
