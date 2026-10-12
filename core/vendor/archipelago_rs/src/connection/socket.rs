// Local patch (see PATCHES.md): the WebSocket layer is soketto with permessage-deflate instead of tungstenite, which has no
// compression ("Uncompressed connections are deprecated and may stop working in the future", Archipelago network protocol).
use std::{
    collections::VecDeque,
    future::{Future, poll_fn},
    io,
    net::TcpStream as SyncTcpStream,
    pin::{Pin, pin},
    sync::Arc,
    task::{Context, Poll, Waker},
};

use futures::io::{AsyncRead, AsyncWrite, BufReader, BufWriter};
use log::*;
#[cfg(feature = "rustls")]
use rustls::{ClientConfig, KeyLogFile, RootCertStore, pki_types::ServerName};
use serde::de::DeserializeOwned;
use smol::{Async, net::TcpStream as AsyncTcpStream};
use soketto::{
    Data,
    connection::{Error as SokettoError, Receiver, Sender},
    extension::deflate::Deflate,
    handshake::{Client as Handshake, ServerResponse},
};
use thiserror::Error as ThisError;

use crate::error::{Error, ProtocolError};
use crate::protocol::{ClientMessage, ServerMessage};

/// The default Archipelago port, used for localhost and potentially other
/// custom servers.
const DEFAULT_PORT: u16 = 38281;

/// Errors of the WebSocket connection itself.
#[derive(ThisError, Debug)]
pub enum WsError {
    /// The connection closed (the server closed it, or it failed earlier).
    #[error("connection closed")]
    ConnectionClosed,

    /// The socket failed.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// The TLS upgrade failed.
    #[error("TLS error: {0}")]
    Tls(String),

    /// The server address could not be understood.
    #[error("invalid URL: {0}")]
    Url(String),

    /// The server did not accept the WebSocket handshake.
    #[error("WebSocket handshake failed: {0}")]
    Handshake(String),

    /// The server broke the WebSocket protocol (framing, compression, message size).
    #[error("WebSocket error: {0}")]
    Protocol(String),
}

impl From<SokettoError> for WsError {
    fn from(err: SokettoError) -> Self {
        match err {
            SokettoError::Closed => WsError::ConnectionClosed,
            SokettoError::Io(err) => WsError::Io(err),
            err => WsError::Protocol(err.to_string()),
        }
    }
}

/// Where to connect, from a `ws://` or `wss://` URL.
#[derive(Debug, PartialEq, Eq)]
struct Target {
    tls: bool,
    /// Host as written (IPv6 in brackets), for the TCP connect and the `Host` header.
    host: String,
    port: u16,
    /// Path and query, at least `/`.
    resource: String,
}

impl Target {
    fn parse(url: &str) -> Result<Self, WsError> {
        let (tls, rest) = if let Some(rest) = url.strip_prefix("wss://") {
            (true, rest)
        } else if let Some(rest) = url.strip_prefix("ws://") {
            (false, rest)
        } else {
            return Err(WsError::Url(format!("not a ws:// or wss:// URL: {url}")));
        };
        let (authority, resource) = rest.find(['/', '?']).map_or((rest, "/"), |i| rest.split_at(i));
        let resource = if resource.starts_with('?') { format!("/{resource}") } else { resource.to_string() };
        // The port follows the last colon, except inside an IPv6 literal (`[::1]`).
        let (host, port) = match authority.rfind(':') {
            Some(i) if !authority[i..].contains(']') => {
                let port = authority[i + 1..].parse().map_err(|_| WsError::Url(format!("bad port in {url}")))?;
                (&authority[..i], port)
            }
            _ => (authority, DEFAULT_PORT),
        };
        if host.is_empty() || host == "[]" {
            return Err(WsError::Url(format!("no host name in {url}")));
        }
        Ok(Target { tls, host: host.to_string(), port, resource })
    }

    /// The host name for TLS (no IPv6 brackets).
    #[cfg(feature = "rustls")]
    fn server_name(&self) -> &str {
        self.host.trim_start_matches('[').trim_end_matches(']')
    }
}

/// The TCP stream, shared with [Readiness] and readable/writable without `&mut`.
#[derive(Clone)]
struct Tcp(Arc<Async<SyncTcpStream>>);

impl AsyncRead for Tcp {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut &*self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for Tcp {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut &*self.0).poll_write(cx, buf)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut &*self.0).poll_flush(cx)
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut &*self.0).poll_close(cx)
    }
}

/// A plain or TLS stream under the WebSocket.
enum Stream {
    Plain(Tcp),
    #[cfg(feature = "rustls")]
    Tls(Box<futures_rustls::client::TlsStream<Tcp>>),
}

impl AsyncRead for Stream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            #[cfg(feature = "rustls")]
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            #[cfg(feature = "rustls")]
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_flush(cx),
            #[cfg(feature = "rustls")]
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_close(cx),
            #[cfg(feature = "rustls")]
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_close(cx),
        }
    }
}

type Io = BufReader<BufWriter<Stream>>;

/// The next message being received: kept across calls and never dropped mid-way (a half-read frame would be lost), polled
/// whenever the host drains the socket. It hands the receiver back with the result.
type Receiving = Pin<Box<dyn Future<Output = (Receiver<Io>, Result<(Vec<u8>, bool), SokettoError>)> + Send>>;

fn receive(mut receiver: Receiver<Io>) -> Receiving {
    Box::pin(async move {
        let mut data = Vec::new();
        let result = receiver.receive_data(&mut data).await.map(|d| (data, matches!(d, Data::Text(_))));
        (receiver, result)
    })
}

/// A WebSocket wrapper that receives Archipelago protocol messages from the
/// server and decodes them.
pub(crate) struct Socket<S: DeserializeOwned + 'static> {
    /// The async wrapper for the TCP stream. We use this to determine when it's
    /// readable and writable.
    async_stream: Arc<Async<SyncTcpStream>>,

    /// Sends messages; pings are answered by the receiver.
    sender: Sender<Io>,

    /// The message being received; `None` once the connection failed or closed.
    receiving: Option<Receiving>,

    /// The buffer of messages that have yet to be returned, in cases where the
    /// server sends multiple messages at a time.
    messages: VecDeque<Result<ServerMessage<S>, Error>>,
}

/// Waits until the socket has data, without borrowing the [Socket] (local patch, see PATCHES.md): a host can sleep until the server
/// sends something instead of polling on a timer, while other threads keep using the connection.
#[derive(Clone)]
pub struct Readiness(Arc<Async<SyncTcpStream>>);

impl Readiness {
    /// A handle for `stream` (made non-blocking), for hosts and tests that hold a socket of their own.
    pub fn new(stream: SyncTcpStream) -> io::Result<Self> {
        Ok(Self(Arc::new(Async::new(stream)?)))
    }

    /// Blocks until the socket is readable (data, a close or an error, all of which the next update reports) or `timeout` passes.
    /// Returns whether it became readable.
    pub fn wait(&self, timeout: std::time::Duration) -> bool {
        smol::block_on(smol::future::or(
            async {
                let _ = self.0.readable().await; // an error is "readable" too: the next update reports it
                true
            },
            async {
                smol::Timer::after(timeout).await;
                false
            },
        ))
    }
}

impl<S: DeserializeOwned + 'static> Socket<S> {
    /// A handle that waits for this socket's next data.
    pub(crate) fn readiness(&self) -> Readiness {
        Readiness(self.async_stream.clone())
    }

    /// Establishes a WebSocket connection to `url` (`ws://` or `wss://`), offering permessage-deflate. A `wss://` URL whose TLS
    /// upgrade fails falls back to plain TCP, as before.
    pub(crate) async fn connect(url: String, #[cfg(feature = "rustls")] rustls_config: Option<Arc<ClientConfig>>) -> Result<Self, Error> {
        let target = Target::parse(&url)?;
        let async_stream = Self::connect_tcp(&target).await?;
        if !target.tls {
            let stream = Stream::Plain(Tcp(async_stream.clone()));
            return Self::upgrade(async_stream, stream, &target).await;
        }
        let (async_stream, stream) = Self::secure(async_stream, &target, #[cfg(feature = "rustls")] rustls_config).await?;
        Self::upgrade(async_stream, stream, &target).await
    }

    /// The stream for a `wss://` URL: TLS, or plain TCP on a new connection when the TLS upgrade fails.
    #[cfg(feature = "rustls")]
    async fn secure(
        async_stream: Arc<Async<SyncTcpStream>>,
        target: &Target,
        rustls_config: Option<Arc<ClientConfig>>,
    ) -> Result<(Arc<Async<SyncTcpStream>>, Stream), Error> {
        match Self::try_tls(&async_stream, target, rustls_config).await {
            Ok(stream) => Ok((async_stream, stream)),
            Err(Error::WebSocket(WsError::Tls(err))) => {
                debug!(
                    "Upgrading to TLS failed: {err}\n\
                     Attempting plain TCP connection..."
                );
                let async_stream = Self::connect_tcp(target).await?;
                let stream = Stream::Plain(Tcp(async_stream.clone()));
                Ok((async_stream, stream))
            }
            Err(err) => Err(err),
        }
    }

    /// Without TLS support a `wss://` URL is plain TCP.
    #[cfg(not(feature = "rustls"))]
    async fn secure(async_stream: Arc<Async<SyncTcpStream>>, _target: &Target) -> Result<(Arc<Async<SyncTcpStream>>, Stream), Error> {
        debug!("No TLS features are enabled, upgrading to unencrypted WebSocket...");
        let stream = Stream::Plain(Tcp(async_stream.clone()));
        Ok((async_stream, stream))
    }

    /// The WebSocket handshake over `stream`, offering permessage-deflate.
    async fn upgrade(async_stream: Arc<Async<SyncTcpStream>>, stream: Stream, target: &Target) -> Result<Self, Error> {
        debug!("Upgrading to WebSocket...");
        let io = BufReader::new(BufWriter::new(stream));
        let authority = format!("{}:{}", target.host, target.port);
        let mut handshake = Handshake::new(io, &authority, &target.resource);
        handshake.add_extension(Box::new(Deflate::new(soketto::Mode::Client)));
        match handshake.handshake().await.map_err(|e| WsError::Handshake(e.to_string()))? {
            ServerResponse::Accepted { .. } => {}
            ServerResponse::Redirect { status_code, location } => {
                return Err(WsError::Handshake(format!("redirected ({status_code}) to {location}")).into());
            }
            ServerResponse::Rejected { status_code } => return Err(WsError::Handshake(format!("rejected ({status_code})")).into()),
        }
        let extensions: Vec<_> = handshake.drain_extensions().collect();
        let compressed = extensions.iter().any(|e| e.is_enabled());
        debug!("WebSocket connected, permessage-deflate: {compressed}");
        let mut builder = handshake.into_builder();
        builder.add_extensions(extensions);
        let (sender, receiver) = builder.finish();
        Ok(Socket { async_stream, sender, receiving: Some(receive(receiver)), messages: Default::default() })
    }

    /// Initializes a new TCP connection to `target`.
    async fn connect_tcp(target: &Target) -> Result<Arc<Async<SyncTcpStream>>, Error> {
        debug!("Establishing TCP connection to {}:{}...", target.host, target.port);
        let host = target.host.trim_start_matches('[').trim_end_matches(']');
        match AsyncTcpStream::connect((host, target.port)).await {
            Ok(stream) => Ok(Arc::<Async<SyncTcpStream>>::from(stream)),
            // Normalize OS errors into the WebSocket error wrapper.
            Err(err) => Err(match err.raw_os_error() {
                Some(os_err) => WsError::Io(io::Error::from_raw_os_error(os_err)).into(),
                None => err.into(),
            }),
        }
    }

    /// Upgrades the TCP stream to TLS using rustls.
    #[cfg(feature = "rustls")]
    async fn try_tls(async_stream: &Arc<Async<SyncTcpStream>>, target: &Target, config: Option<Arc<ClientConfig>>) -> Result<Stream, Error> {
        debug!("Upgrading to TLS using rustls...");
        let config = config.unwrap_or_else(|| {
            let mut root_store = RootCertStore::empty();
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let mut config = ClientConfig::builder().with_root_certificates(root_store).with_no_client_auth();
            config.key_log = Arc::new(KeyLogFile::new());
            Arc::new(config)
        });
        let name = ServerName::try_from(target.server_name().to_string()).map_err(|_| WsError::Tls("invalid DNS name".into()))?;
        let stream = futures_rustls::TlsConnector::from(config)
            .connect(name, Tcp(async_stream.clone()))
            .await
            .map_err(|e| WsError::Tls(e.to_string()))?;
        Ok(Stream::Tls(Box::new(stream)))
    }

    /// Returns the list of all messages and errors queued in this socket.
    /// Returns an empty list if the socket has received no data since the last
    /// time this was called.
    ///
    /// The list will contain any errors encountered while processing the
    /// incoming data, including errors for unknown messages either at the
    /// Archipelago or WebSocket levels. There's no guarantee that it will
    /// contain only one error. This never blocks.
    pub(crate) fn recv_all(&mut self) -> impl IntoIterator<Item = Result<ServerMessage<S>, Error>> + use<S> {
        self.read_inner();
        std::mem::take(&mut self.messages)
    }

    /// Returns the next result from this socket, if one is available. Returns
    /// `None` if the socket has received no data since the last time this was
    /// called.
    ///
    /// This returns errors interleaved with messages in the order they were
    /// encountered. This never blocks.
    pub(crate) fn try_recv(&mut self) -> Option<Result<ServerMessage<S>, Error>> {
        self.read_inner();
        self.messages.pop_front()
    }

    /// Like [try_recv], but returns a Future that only resolves once a result
    /// is available.
    pub(crate) async fn recv_async(&mut self) -> Result<ServerMessage<S>, Error> {
        loop {
            match self.try_recv() {
                Some(result) => return result,
                None if self.receiving.is_none() => return Err(WsError::ConnectionClosed.into()),
                None => self.async_stream.readable().await?,
            }
        }
    }

    /// Processes everything the socket has now (answering pings on the way) into [Self::messages], without blocking.
    ///
    /// This can't fail because any errors it encounters will be put in
    /// [Self::messages] rather than returned directly.
    fn read_inner(&mut self) {
        pump(&mut self.receiving, &mut self.messages, &mut Context::from_waker(Waker::noop()));
    }

    /// Sends [message] to the server, waiting at most [SEND_TIMEOUT] for it to go out. A send that fails or times out may have left
    /// half a frame on the wire, so the connection ends: the next update reports it and the host reconnects.
    pub(crate) fn send(&mut self, message: ClientMessage) -> Result<(), Error> {
        if self.receiving.is_none() {
            return Err(WsError::ConnectionClosed.into());
        }
        let text = serde_json::to_string(&[&message]).map_err(Error::Serialize)?;
        debug!("<-- {text}");
        let Self { sender, receiving, messages, .. } = self;
        let mut sent = pin!(async {
            sender.send_text(&text).await?;
            sender.flush().await
        });
        // The receiver may hold the writer (a pong half written): keep it going until the send is through.
        let sending = poll_fn(|cx| match sent.as_mut().poll(cx) {
            Poll::Ready(result) => Poll::Ready(result.map_err(WsError::from)),
            Poll::Pending => {
                pump(receiving, messages, cx);
                Poll::Pending
            }
        });
        let deadline = async {
            smol::Timer::after(SEND_TIMEOUT).await;
            Err(WsError::Io(io::Error::new(io::ErrorKind::TimedOut, "the server took no data for the send timeout")))
        };
        let result = smol::block_on(smol::future::or(sending, deadline));
        if let Err(err) = &result {
            debug!("<-- [error] {err}: closing the connection");
            self.receiving = None;
            self.messages.push_back(Err(WsError::Io(io::Error::other(format!("send failed: {err}"))).into()));
        }
        result.map_err(Error::WebSocket)
    }
}

/// How long a send may wait for the server to take its data: past that the network or server is dead (a quiet live connection
/// takes a frame at once), and blocking longer would hold the connection until the OS gives up minutes later.
const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Polls `receiving` until it has to wait, decoding each message into `messages`. Any WebSocket error ends the connection.
fn pump<S: DeserializeOwned + 'static>(
    receiving: &mut Option<Receiving>,
    messages: &mut VecDeque<Result<ServerMessage<S>, Error>>,
    cx: &mut Context<'_>,
) {
    while let Some(next) = receiving.as_mut() {
        let Poll::Ready((receiver, result)) = next.as_mut().poll(cx) else { return };
        match result {
            Ok((data, true)) => {
                debug!("--> {}", String::from_utf8_lossy(&data));
                match serde_json::from_slice::<Vec<ServerMessage<S>>>(&data) {
                    Ok(decoded) => messages.extend(decoded.into_iter().map(Ok)),
                    Err(error) => {
                        messages.push_back(Err(ProtocolError::Deserialize { json: String::from_utf8_lossy(&data).into_owned(), error }.into()))
                    }
                }
                *receiving = Some(receive(receiver));
            }
            Ok((data, false)) => {
                messages.push_back(Err(ProtocolError::BinaryMessage(data).into()));
                *receiving = Some(receive(receiver));
            }
            Err(err) => {
                debug!("--> [error] {err:?}");
                messages.push_back(Err(Error::WebSocket(err.into())));
                *receiving = None;
            }
        }
    }
}
