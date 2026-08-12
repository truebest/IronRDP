//! Asks a server for HEVC over the graphics pipeline and writes what comes back.
//!
//! This is the reference consumer of the HEVC extension: it advertises the private
//! capability version with the HEVC flag, takes every `WireToSurface1` PDU that arrives
//! under the HEVC codec id, and appends the Annex-B access units to a file. Nothing is
//! decoded here — the point is to prove the wire format, so the file is meant to be fed
//! to `ffprobe`/`ffmpeg` afterwards.
//!
//! ```shell
//! cargo run -p ironrdp-hevc-probe -- --host <HOST> -u <USER> -o out.h265 --seconds 15
//! ```
//!
//! The password is read from stdin when `-p` is omitted, so it never lands in the
//! process arguments.

#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]

use core::time::Duration;
use std::fs::File;
use std::io::{BufWriter, Write as _};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Context as _;
use ironrdp::connector::{self, ConnectionResult, Credentials};
use ironrdp::dvc::DrdynvcClient;
use ironrdp::pdu::gcc::KeyboardType;
use ironrdp::pdu::rdp::capability_sets::MajorPlatformType;
use ironrdp::session::image::DecodedImage;
use ironrdp::session::{ActiveStageBuilder, ActiveStageOutput};
use ironrdp_egfx::client::{GraphicsPipelineClient, GraphicsPipelineHandler};
use ironrdp_egfx::pdu::{CapabilitiesFrdp1Flags, CapabilitiesV8Flags, CapabilitySet};
use ironrdp_pdu::rdp::client_info::{PerformanceFlags, TimezoneInfo};
use sspi::network_client::reqwest_network_client::ReqwestNetworkClient;
use tokio_rustls::rustls;
use tracing::{debug, info};

const HELP: &str = "\
USAGE:
  ironrdp-hevc-probe --host <HOSTNAME> [--port <PORT>]
                     -u/--username <USERNAME> [-p/--password <PASSWORD>]
                     [-o/--output <OUTPUT_FILE>] [-d/--domain <DOMAIN>]
                     [--seconds <SECONDS>] [--allow-avc]

  The password is read from stdin when -p is omitted.
  --allow-avc keeps AVC available, so the server may answer with H.264 instead.
";

struct Stats {
    frames: AtomicU64,
    bytes: AtomicU64,
}

/// Advertises HEVC and writes every access unit it is handed.
struct HevcProbeHandler {
    output: Arc<Mutex<BufWriter<File>>>,
    stats: Arc<Stats>,
    allow_avc: bool,
}

impl GraphicsPipelineHandler for HevcProbeHandler {
    fn capabilities(&self) -> Vec<CapabilitySet> {
        let mut flags = CapabilitiesFrdp1Flags::HEVC_SUPPORTED | CapabilitiesFrdp1Flags::SMALL_CACHE;

        // Without a decoder for it, offering AVC only invites the server to send a
        // stream this tool cannot check.
        if !self.allow_avc {
            flags |= CapabilitiesFrdp1Flags::AVC_DISABLED;
        }

        vec![
            CapabilitySet::Frdp1 { flags },
            // Fallback for a server that does not know the extension: it then picks a
            // standard version and sends progressive, which this tool ignores.
            CapabilitySet::V8 {
                flags: CapabilitiesV8Flags::SMALL_CACHE,
            },
        ]
    }

    fn on_capabilities_confirmed(&mut self, caps: &CapabilitySet) {
        println!("server confirmed capability set {caps:?}");

        if !matches!(caps, CapabilitySet::Frdp1 { flags } if flags.contains(CapabilitiesFrdp1Flags::HEVC_SUPPORTED)) {
            eprintln!("warning: the server did not accept the HEVC extension");
        }
    }

    fn on_hevc_frame(&mut self, surface_id: u16, left: u16, top: u16, width: u16, height: u16, nal: &[u8]) -> bool {
        debug!(surface_id, left, top, width, height, len = nal.len(), "HEVC access unit");

        self.stats.frames.fetch_add(1, Ordering::Relaxed);
        self.stats.bytes.fetch_add(nal.len() as u64, Ordering::Relaxed);

        let mut output = self.output.lock().expect("output mutex");
        output.write_all(nal).expect("write access unit");

        true
    }
}

fn main() -> anyhow::Result<()> {
    let action = match parse_args() {
        Ok(action) => action,
        Err(e) => {
            println!("{HELP}");
            return Err(e.context("invalid argument(s)"));
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Two providers are reachable through the dependency graph, so name the one to use.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install the default crypto provider");

    match action {
        Action::ShowHelp => {
            println!("{HELP}");
            Ok(())
        }
        Action::Run(config) => run(config),
    }
}

struct RunConfig {
    host: String,
    port: u16,
    username: String,
    password: String,
    domain: Option<String>,
    output: PathBuf,
    seconds: u64,
    allow_avc: bool,
}

enum Action {
    ShowHelp,
    Run(RunConfig),
}

fn parse_args() -> anyhow::Result<Action> {
    let mut args = pico_args::Arguments::from_env();

    if args.contains(["-h", "--help"]) {
        return Ok(Action::ShowHelp);
    }

    let host: String = args.value_from_str("--host")?;
    let port: u16 = args.opt_value_from_str("--port")?.unwrap_or(3389);
    let username: String = args.value_from_str(["-u", "--username"])?;
    let password: Option<String> = args.opt_value_from_str(["-p", "--password"])?;
    let domain: Option<String> = args.opt_value_from_str(["-d", "--domain"])?;
    let output: PathBuf = args
        .opt_value_from_str(["-o", "--output"])?
        .unwrap_or_else(|| PathBuf::from("hevc-probe.h265"));
    let seconds: u64 = args.opt_value_from_str("--seconds")?.unwrap_or(15);
    let allow_avc = args.contains("--allow-avc");

    let password = match password {
        Some(password) => password,
        None => {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).context("read password")?;
            line.trim_end_matches(['\r', '\n']).to_owned()
        }
    };

    Ok(Action::Run(RunConfig {
        host,
        port,
        username,
        password,
        domain,
        output,
        seconds,
        allow_avc,
    }))
}

fn run(config: RunConfig) -> anyhow::Result<()> {
    let output = Arc::new(Mutex::new(BufWriter::new(
        File::create(&config.output).context("create output file")?,
    )));
    let stats = Arc::new(Stats {
        frames: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
    });

    let handler = HevcProbeHandler {
        output: Arc::clone(&output),
        stats: Arc::clone(&stats),
        allow_avc: config.allow_avc,
    };

    let connector_config = build_config(config.username, config.password, config.domain);
    let (connection_result, framed) =
        connect(connector_config, &config.host, config.port, handler).context("connect")?;

    println!(
        "connected, desktop {}x{}, collecting for {} s",
        connection_result.desktop_size.width, connection_result.desktop_size.height, config.seconds
    );

    let mut image = DecodedImage::new(
        ironrdp_graphics::image_processing::PixelFormat::RgbA32,
        connection_result.desktop_size.width,
        connection_result.desktop_size.height,
    );

    active_stage(connection_result, framed, &mut image, config.seconds).context("active stage")?;

    output.lock().expect("output mutex").flush().context("flush output")?;

    let frames = stats.frames.load(Ordering::Relaxed);
    let bytes = stats.bytes.load(Ordering::Relaxed);
    println!(
        "wrote {frames} HEVC access units, {bytes} bytes, to {}",
        config.output.display()
    );

    if frames == 0 {
        anyhow::bail!("no HEVC frame arrived");
    }

    Ok(())
}

fn build_config(username: String, password: String, domain: Option<String>) -> connector::Config {
    connector::Config {
        credentials: Credentials::UsernamePassword { username, password },
        domain,
        enable_tls: false,
        enable_credssp: true,
        keyboard_type: KeyboardType::IbmEnhanced,
        keyboard_subtype: 0,
        keyboard_layout: 0,
        keyboard_functional_keys_count: 12,
        ime_file_name: String::new(),
        dig_product_id: String::new(),
        desktop_size: connector::DesktopSize {
            width: 1920,
            height: 1080,
        },
        bitmap: None,
        client_build: 0,
        client_name: "ironrdp-hevc-probe".to_owned(),
        client_dir: "C:\\Windows\\System32\\mstscax.dll".to_owned(),
        platform: MajorPlatformType::UNIX,
        enable_server_pointer: false,
        request_data: None,
        autologon: false,
        enable_audio_playback: false,
        enable_audio_capture: false,
        compression_type: None,
        pointer_software_rendering: true,
        multitransport_flags: None,
        performance_flags: PerformanceFlags::default(),
        desktop_scale_factor: 0,
        hardware_id: None,
        license_cache: None,
        timezone_info: TimezoneInfo::default(),
        alternate_shell: String::new(),
        work_dir: String::new(),
    }
}

type UpgradedFramed = ironrdp_blocking::Framed<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>;

fn connect(
    config: connector::Config,
    server_name: &str,
    port: u16,
    handler: HevcProbeHandler,
) -> anyhow::Result<(ConnectionResult, UpgradedFramed)> {
    let server_addr = lookup_addr(server_name, port).context("lookup addr")?;

    info!(%server_addr, "Looked up server address");

    let tcp_stream = TcpStream::connect(server_addr).context("TCP connect")?;

    // Bounds the blocking read in the active stage, so the collection deadline is
    // checked even while the server is quiet.
    tcp_stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set_read_timeout call failed");

    let client_addr = tcp_stream.local_addr().context("get socket local address")?;

    let mut framed = ironrdp_blocking::Framed::new(tcp_stream);

    let mut connector = connector::ClientConnector::new(config, client_addr);
    connector.attach_static_channel(
        DrdynvcClient::new().with_dynamic_channel(GraphicsPipelineClient::new(Box::new(handler), None)),
    );

    let should_upgrade = ironrdp_blocking::connect_begin(&mut framed, &mut connector).context("begin connection")?;

    debug!("TLS upgrade");

    let initial_stream = framed.into_inner_no_leftover();

    let (upgraded_stream, server_public_key) =
        tls_upgrade(initial_stream, server_name.to_owned()).context("TLS upgrade")?;

    let upgraded = ironrdp_blocking::mark_as_upgraded(should_upgrade, &mut connector);

    let mut upgraded_framed = ironrdp_blocking::Framed::new(upgraded_stream);

    let mut network_client = ReqwestNetworkClient;
    let connection_result = ironrdp_blocking::connect_finalize(
        upgraded,
        connector,
        &mut upgraded_framed,
        &mut network_client,
        server_name.to_owned().into(),
        server_public_key,
        None,
    )
    .context("finalize connection")?;

    Ok((connection_result, upgraded_framed))
}

fn active_stage(
    connection_result: ConnectionResult,
    mut framed: UpgradedFramed,
    image: &mut DecodedImage,
    seconds: u64,
) -> anyhow::Result<()> {
    let mut active_stage = ActiveStageBuilder {
        static_channels: connection_result.static_channels,
        user_channel_id: connection_result.user_channel_id,
        io_channel_id: connection_result.io_channel_id,
        message_channel_id: connection_result.message_channel_id,
        share_id: connection_result.share_id,
        compression_type: connection_result.compression_type,
        enable_server_pointer: connection_result.enable_server_pointer,
        pointer_software_rendering: connection_result.pointer_software_rendering,
    }
    .build();

    let deadline = Instant::now() + Duration::from_secs(seconds);

    'outer: loop {
        if Instant::now() >= deadline {
            break 'outer;
        }

        let (action, payload) = match framed.read_pdu() {
            Ok((action, payload)) => (action, payload),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                continue 'outer;
            }
            Err(e) => return Err(anyhow::Error::new(e).context("read frame")),
        };

        let outputs = active_stage.process(image, action, &payload)?;

        for out in outputs {
            match out {
                ActiveStageOutput::ResponseFrame(frame) => framed.write_all(&frame).context("write response")?,
                ActiveStageOutput::Terminate(_) => break 'outer,
                _ => {}
            }
        }
    }

    Ok(())
}

fn lookup_addr(hostname: &str, port: u16) -> anyhow::Result<core::net::SocketAddr> {
    use std::net::ToSocketAddrs as _;
    let addr = (hostname, port)
        .to_socket_addrs()?
        .next()
        .context("socket address not found")?;
    Ok(addr)
}

fn tls_upgrade(
    stream: TcpStream,
    server_name: String,
) -> anyhow::Result<(rustls::StreamOwned<rustls::ClientConnection, TcpStream>, Vec<u8>)> {
    let mut config = rustls::client::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(danger::NoCertificateVerification))
        .with_no_client_auth();

    config.key_log = std::sync::Arc::new(rustls::KeyLogFile::new());
    config.resumption = rustls::client::Resumption::disabled();

    let config = std::sync::Arc::new(config);

    let server_name = server_name.try_into()?;

    let client = rustls::ClientConnection::new(config, server_name)?;

    let mut tls_stream = rustls::StreamOwned::new(client, stream);

    tls_stream.flush()?;

    let cert = tls_stream
        .conn
        .peer_certificates()
        .and_then(|certificates| certificates.first())
        .context("peer certificate is missing")?;

    let server_public_key = extract_tls_server_public_key(cert)?;

    Ok((tls_stream, server_public_key))
}

fn extract_tls_server_public_key(cert: &[u8]) -> anyhow::Result<Vec<u8>> {
    use x509_cert::der::Decode as _;

    let cert = x509_cert::Certificate::from_der(cert)?;

    let server_public_key = cert
        .tbs_certificate
        .subject_public_key_info
        .subject_public_key
        .as_bytes()
        .context("subject public key BIT STRING is not aligned")?
        .to_owned();

    Ok(server_public_key)
}

mod danger {
    use tokio_rustls::rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use tokio_rustls::rustls::{self, DigitallySignedStruct, Error, SignatureScheme};

    #[derive(Debug)]
    pub(super) struct NoCertificateVerification;

    impl ServerCertVerifier for NoCertificateVerification {
        fn verify_server_cert(
            &self,
            _: &CertificateDer<'_>,
            _: &[CertificateDer<'_>],
            _: &ServerName<'_>,
            _: &[u8],
            _: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _: &[u8],
            _: &CertificateDer<'_>,
            _: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _: &[u8],
            _: &CertificateDer<'_>,
            _: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            rustls::crypto::ring::default_provider()
                .signature_verification_algorithms
                .supported_schemes()
        }
    }
}
