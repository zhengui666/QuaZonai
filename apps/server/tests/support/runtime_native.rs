//! Shared actual TCP/TLS fixture. No production runtime or OCI acceptance claim.
#![allow(dead_code)]
#[path = "../../../../tests/support/runtime.rs"]
mod fixture;
use axum::{
    body::{Body, Bytes},
    http::{header, HeaderMap, Response, StatusCode},
    routing::get,
    Router,
};
use std::{
    convert::Infallible,
    fs,
    net::SocketAddr,
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::JoinHandle,
};

pub const SECRET: &str = "runtime-native-credential-sentinel-AL1z";
pub use fixture::capabilities;

pub struct NativeServer {
    pub address: SocketAddr,
    pub requests: Arc<AtomicUsize>,
    task: JoinHandle<()>,
}
impl Drop for NativeServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
pub async fn native_http(
    status: StatusCode,
    payload: Vec<u8>,
    redirect: Option<String>,
    chunked: bool,
) -> NativeServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let app = Router::new().route(
        "/runtime/v1/capabilities",
        get(move |headers: HeaderMap| {
            let count = count.clone();
            let payload = payload.clone();
            let redirect = redirect.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                assert!(
                    headers[header::AUTHORIZATION] == format!("Bearer {SECRET}"),
                    "native credential header mismatch"
                );
                assert!(headers.get(header::COOKIE).is_none());
                let mut response = Response::builder()
                    .status(status)
                    .header(header::CONTENT_TYPE, "application/json");
                if let Some(location) = redirect {
                    response = response.header(header::LOCATION, location);
                }
                let body = if chunked {
                    let pieces: Vec<Result<Bytes, Infallible>> = payload
                        .chunks(32768)
                        .map(|piece| Ok(Bytes::copy_from_slice(piece)))
                        .collect();
                    Body::from_stream(futures_util::stream::iter(pieces))
                } else {
                    Body::from(payload)
                };
                response.body(body).unwrap()
            }
        }),
    );
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    NativeServer {
        address,
        requests,
        task,
    }
}

pub struct NativeTls {
    pub server: NativeServer,
    pub ca: Vec<u8>,
    _files: tempfile::TempDir,
}
impl NativeTls {
    pub fn endpoint(&self) -> String {
        format!(
            "https://runtime-native.invalid:{}",
            self.server.address.port()
        )
    }
}
fn openssl(root: &std::path::Path, args: &[&str]) {
    let result = Command::new("openssl")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "native certificate fixture generation failed"
    );
}
pub async fn native_tls() -> NativeTls {
    native_tls_with_barrier(None, None).await
}

pub async fn native_tls_concurrent() -> NativeTls {
    native_tls_with_barrier(Some(Arc::new(tokio::sync::Barrier::new(2))), None).await
}

struct CatalogReply {
    target: String,
    payload: Vec<u8>,
    pause: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
}

pub async fn native_tls_catalog(
    target: String,
    payload: Vec<u8>,
    pause: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
) -> NativeTls {
    native_tls_with_barrier(
        None,
        Some(Arc::new(CatalogReply {
            target,
            payload,
            pause,
        })),
    )
    .await
}

async fn native_tls_with_barrier(
    barrier: Option<Arc<tokio::sync::Barrier>>,
    catalog: Option<Arc<CatalogReply>>,
) -> NativeTls {
    let files = tempfile::tempdir().unwrap();
    let root = files.path();
    openssl(
        root,
        &[
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-nodes",
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
            "-days",
            "1",
            "-subj",
            "/CN=QuaZonai test CA",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
        ],
    );
    openssl(
        root,
        &[
            "req",
            "-new",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-nodes",
            "-keyout",
            "server.key",
            "-out",
            "server.csr",
            "-subj",
            "/CN=runtime-native.invalid",
        ],
    );
    fs::write(root.join("extensions.cnf"), "subjectAltName=DNS:runtime-native.invalid\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\n").unwrap();
    openssl(
        root,
        &[
            "x509",
            "-req",
            "-in",
            "server.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-CAcreateserial",
            "-out",
            "server.pem",
            "-days",
            "1",
            "-extfile",
            "extensions.cnf",
        ],
    );
    openssl(
        root,
        &[
            "x509",
            "-in",
            "server.pem",
            "-outform",
            "DER",
            "-out",
            "server.der",
        ],
    );
    openssl(
        root,
        &[
            "pkcs8",
            "-topk8",
            "-nocrypt",
            "-in",
            "server.key",
            "-outform",
            "DER",
            "-out",
            "server-key.der",
        ],
    );
    let ca = fs::read(root.join("ca.pem")).unwrap();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![rustls::pki_types::CertificateDer::from(
            fs::read(root.join("server.der")).unwrap(),
        )],
        rustls::pki_types::PrivatePkcs8KeyDer::from(fs::read(root.join("server-key.der")).unwrap())
            .into(),
    )
    .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let task = tokio::spawn(async move {
        // The owner retains every connection task; aborting the fixture drops
        // this JoinSet and cancels its children instead of detaching listeners.
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept(), if connections.len() < 16 => {
                    let (socket, _) = accepted.unwrap();
                    let acceptor = acceptor.clone();
                    let count = count.clone();
                    let barrier = barrier.clone();
                    let catalog = catalog.clone();
                    connections.spawn(async move {
                        tokio::time::timeout(std::time::Duration::from_secs(10), async move {
                            let Ok(mut socket) = acceptor.accept(socket).await else {
                                return;
                            };
                            let mut request = Vec::new();
                            loop {
                                let byte = socket.read_u8().await.unwrap();
                                request.push(byte);
                                assert!(request.len() <= 8192);
                                if request.ends_with(b"\r\n\r\n") { break; }
                            }
                            let request = String::from_utf8(request).unwrap();
                            let probing = request.starts_with("GET /runtime/v1/capabilities HTTP/1.1\r\n");
                            let expected_target = catalog.as_ref().map_or("/runtime/v1/capabilities", |reply| reply.target.as_str());
                            assert!(probing || request.starts_with(&format!("GET {expected_target} HTTP/1.1\r\n")));
                            assert!(request.to_ascii_lowercase().contains("host: runtime-native.invalid:"));
                            assert!(request.contains(&format!("authorization: Bearer {SECRET}")));
                            count.fetch_add(1, Ordering::SeqCst);
                            if let Some(barrier) = barrier { barrier.wait().await; }
                            let payload = if probing {
                                let mut observed = capabilities(chrono::Utc::now());
                                if catalog.is_some() {
                                    observed.artifact_schemas.push(contracts::runtime::RuntimeArtifactSchemaV1 {
                                        name: "qz.data_quality".into(), version: "1".into(),
                                    });
                                }
                                serde_json::to_vec(&observed).unwrap()
                            } else {
                                let reply = catalog.expect("only the exact registered catalog endpoint is accepted");
                                if let Some((entered, release)) = &reply.pause {
                                    entered.notify_one();
                                    release.notified().await;
                                }
                                reply.payload.clone()
                            };
                            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len()).as_bytes()).await.unwrap();
                            socket.write_all(&payload).await.unwrap();
                            socket.shutdown().await.unwrap();
                        }).await.expect("native TLS fixture connection deadline");
                    });
                }
                completed = connections.join_next(), if !connections.is_empty() => {
                    completed.unwrap().expect("native TLS fixture connection failed");
                }
            }
        }
    });
    NativeTls {
        server: NativeServer {
            address,
            requests,
            task,
        },
        ca,
        _files: files,
    }
}
