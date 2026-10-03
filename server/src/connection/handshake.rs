use crate::conn_limit::forwarded_client_ip;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::OnceLock;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

const MAX_CAPTURE_BYTES: usize = 16 * 1024;

pub(super) struct HandshakeStream<'a, S> {
    stream: S,
    peer: SocketAddr,
    client_ip: &'a OnceLock<IpAddr>,
    headers: Option<Vec<u8>>,
}

impl<'a, S> HandshakeStream<'a, S> {
    pub(super) fn new(stream: S, peer: SocketAddr, client_ip: &'a OnceLock<IpAddr>) -> Self {
        Self {
            stream,
            peer,
            client_ip,
            headers: peer.ip().is_loopback().then(Vec::new),
        }
    }

    fn capture(&mut self, bytes: &[u8]) {
        let Some(buffer) = &mut self.headers else {
            return;
        };
        let remaining = MAX_CAPTURE_BYTES - buffer.len();
        buffer.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
        let mut headers = [httparse::EMPTY_HEADER; 124];
        let mut request = httparse::Request::new(&mut headers);
        match request.parse(buffer) {
            Ok(httparse::Status::Complete(_)) => {
                let header = |name: &str| {
                    request.headers.iter().rev().find_map(|header| {
                        header
                            .name
                            .eq_ignore_ascii_case(name)
                            .then(|| std::str::from_utf8(header.value).ok())
                            .flatten()
                    })
                };
                let ip =
                    forwarded_client_ip(self.peer, header("x-forwarded-for"), header("x-real-ip"));
                let _ = self.client_ip.set(ip);
            }
            Ok(httparse::Status::Partial) if buffer.len() < MAX_CAPTURE_BYTES => return,
            _ => {}
        }
        self.headers = None;
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for HandshakeStream<'_, S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let result = Pin::new(&mut this.stream).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            this.capture(&buf.filled()[before..]);
        }
        result
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for HandshakeStream<'_, S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().stream).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio::io::AsyncWriteExt;
    use tokio_tungstenite::{accept_async, client_async, tungstenite::Message};

    #[test]
    fn fragmented_headers_are_captured_once_without_retaining_frames() {
        let ip = OnceLock::new();
        let mut stream = HandshakeStream::new((), "127.0.0.1:1234".parse().unwrap(), &ip);
        stream.capture(b"GET /ws HTTP/1.1\r\nX-Forwarded-For: 192.0.2.1, 203.");
        assert!(ip.get().is_none());
        stream.capture(b"0.113.7\r\n\r\n");
        assert_eq!(ip.get(), Some(&"203.0.113.7".parse().unwrap()));
        assert!(stream.headers.is_none());
        stream.capture(b"GET /ws HTTP/1.1\r\nX-Forwarded-For: 192.0.2.9\r\n\r\n");
        assert_eq!(ip.get(), Some(&"203.0.113.7".parse().unwrap()));
    }

    #[test]
    fn oversized_or_invalid_headers_stop_capture() {
        for request in [
            b"invalid\r\n\r\n".to_vec(),
            vec![b'x'; MAX_CAPTURE_BYTES + 1],
        ] {
            let ip = OnceLock::new();
            let mut stream = HandshakeStream::new((), "127.0.0.1:1234".parse().unwrap(), &ip);
            stream.capture(&request);
            assert!(stream.headers.is_none());
            assert!(ip.get().is_none());
        }
    }

    #[tokio::test]
    async fn rejected_upgrades_still_capture_proxy_ip() {
        for method in ["GET", "POST"] {
            let ip = OnceLock::new();
            let (mut client, server) = tokio::io::duplex(4096);
            let request = format!(
                "{method} /ws HTTP/1.1\r\nHost: localhost\r\nX-Forwarded-For: 192.0.2.1, 203.0.113.7\r\n\r\n"
            );
            client.write_all(request.as_bytes()).await.unwrap();
            let stream = HandshakeStream::new(server, "127.0.0.1:1234".parse().unwrap(), &ip);
            assert!(accept_async(stream).await.is_err());
            assert_eq!(ip.get(), Some(&"203.0.113.7".parse().unwrap()));
        }
    }

    #[tokio::test]
    async fn successful_upgrade_preserves_websocket_reads_and_writes() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;

        let ip = OnceLock::new();
        let (client, server) = tokio::io::duplex(4096);
        let mut request = "ws://localhost/ws".into_client_request().unwrap();
        request
            .headers_mut()
            .insert("x-real-ip", "203.0.113.7".parse().unwrap());
        let stream = HandshakeStream::new(server, "127.0.0.1:1234".parse().unwrap(), &ip);
        let (server, client) = tokio::join!(accept_async(stream), client_async(request, client));
        let mut server = server.unwrap();
        let (mut client, _) = client.unwrap();
        assert_eq!(ip.get(), Some(&"203.0.113.7".parse().unwrap()));
        client.send(Message::Text("client".into())).await.unwrap();
        assert_eq!(
            server.next().await.unwrap().unwrap(),
            Message::Text("client".into())
        );
        server.send(Message::Text("server".into())).await.unwrap();
        assert_eq!(
            client.next().await.unwrap().unwrap(),
            Message::Text("server".into())
        );
    }
}
