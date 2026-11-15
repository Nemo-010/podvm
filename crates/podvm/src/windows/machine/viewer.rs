//! TODO/machine.md T-1899: `podvm windows view NAME [--port P]`, the
//! screen of a running machine in a browser. QEMU serves VNC on a Unix
//! socket of the machine; this module serves, on 127.0.0.1 only, a page
//! and a WebSocket bridge (`/websockify`, the path of noVNC) that joins
//! each client to that socket. The bridge passes the RFB bytes as they
//! are, as `websockify` does.
//!
//! ⛔ The viewer listens on the loopback only (the Decision of T-1899): a
//! screen of a guest is a session with no password.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use podvm_image::error::{EXIT_FLAG_ERROR, EXIT_RUNTIME_ERROR};
use podvm_windows::machines;

use super::find;

/// The key of RFC 6455 that the accept value of a handshake hashes with.
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// The `Sec-WebSocket-Accept` value of a `Sec-WebSocket-Key` (RFC 6455,
/// section 4.2.2).
pub(crate) fn accept_key(key: &str) -> String {
    use base64::Engine;
    use sha1::{Digest, Sha1};
    let digest = Sha1::digest(format!("{}{WS_GUID}", key.trim()).as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

/// One binary frame from the server: no mask (RFC 6455, section 5.1).
pub(crate) fn frame(payload: &[u8]) -> Vec<u8> {
    let mut f = vec![0x82];
    match payload.len() {
        n if n < 126 => f.push(n as u8),
        n if n <= 0xffff => {
            f.push(126);
            f.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            f.push(127);
            f.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    f.extend_from_slice(payload);
    f
}

/// One frame from a client: its opcode and its unmasked payload, or `None`
/// at the end of the stream or for a frame with no mask (RFC 6455 refuses
/// an unmasked frame from a client).
pub(crate) fn read_frame(r: &mut impl Read) -> Option<(u8, Vec<u8>)> {
    let mut h = [0u8; 2];
    r.read_exact(&mut h).ok()?;
    let opcode = h[0] & 0x0f;
    if h[1] & 0x80 == 0 {
        return None;
    }
    let len = match h[1] & 0x7f {
        126 => {
            let mut b = [0u8; 2];
            r.read_exact(&mut b).ok()?;
            u64::from(u16::from_be_bytes(b))
        }
        127 => {
            let mut b = [0u8; 8];
            r.read_exact(&mut b).ok()?;
            u64::from_be_bytes(b)
        }
        n => u64::from(n),
    };
    // A screen update is small; a frame of more than 16 MiB is no client.
    if len > 16 << 20 {
        return None;
    }
    let mut mask = [0u8; 4];
    r.read_exact(&mut mask).ok()?;
    let mut p = vec![0u8; len as usize];
    r.read_exact(&mut p).ok()?;
    for (i, b) in p.iter_mut().enumerate() {
        *b ^= mask[i % 4];
    }
    Some((opcode, p))
}

/// The page at `/`: the noVNC client of the store where podvm has it, else
/// the address of the bridge for a client of the user.
fn page(port: u16, name: &str, novnc: bool) -> String {
    if novnc {
        return format!(
            "<!doctype html><meta charset=utf-8><title>podvm {name}</title>\
             <style>html,body{{margin:0;height:100%;background:#202020}}#s{{height:100%}}</style>\
             <div id=s></div><script type=module>\
             import RFB from './novnc/core/rfb.js';\
             const r = new RFB(document.getElementById('s'), 'ws://' + location.host + '/websockify');\
             r.scaleViewport = true; r.focusOnClick = true;\
             </script>"
        );
    }
    format!(
        "<!doctype html><meta charset=utf-8><title>podvm {name}</title>\
         <h1>podvm: the screen of {name}</h1>\
         <p>The VNC stream of the guest is the WebSocket \
         <code>ws://127.0.0.1:{port}/websockify</code>. Open it with a noVNC \
         client (host 127.0.0.1, port {port}, path websockify), or with any \
         VNC viewer that takes a WebSocket.</p>"
    )
}

/// Serve one connection: the page, or the bridge to `vnc`.
fn serve(mut c: TcpStream, vnc: &std::path::Path, port: u16, name: &str) {
    let _ = c.set_read_timeout(Some(Duration::from_secs(10)));
    let mut head = Vec::new();
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") && head.len() < 8192 {
        match c.read(&mut b) {
            Ok(1) => head.push(b[0]),
            _ => return,
        }
    }
    let text = String::from_utf8_lossy(&head).to_string();
    let key = text.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim()
            .eq_ignore_ascii_case("sec-websocket-key")
            .then(|| v.trim().to_string())
    });
    let Some(key) = key else {
        let path = text.split_whitespace().nth(1).unwrap_or("/");
        let novnc = podvm_windows::tools::novnc_root();
        let (status, kind, body) = match path.strip_prefix("/novnc/") {
            Some(rest) => match static_file(&novnc, rest) {
                Some((kind, bytes)) => ("200 OK", kind, bytes),
                None => ("404 Not Found", "text/plain", b"not found\n".to_vec()),
            },
            None => (
                "200 OK",
                "text/html",
                page(port, name, novnc.join("core/rfb.js").is_file()).into_bytes(),
            ),
        };
        let _ = write!(
            c,
            "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = c.write_all(&body);
        return;
    };
    let Ok(mut v) = UnixStream::connect(vnc) else {
        let _ = c.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n");
        return;
    };
    let proto = if text
        .to_ascii_lowercase()
        .contains("sec-websocket-protocol: binary")
    {
        "Sec-WebSocket-Protocol: binary\r\n"
    } else {
        ""
    };
    if write!(
        c,
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n{proto}\r\n",
        accept_key(&key)
    )
    .is_err()
    {
        return;
    }
    let _ = c.set_read_timeout(None);
    let (Ok(mut c2), Ok(mut v2)) = (c.try_clone(), v.try_clone()) else {
        return;
    };
    // The guest to the browser.
    let down = std::thread::spawn(move || {
        let mut buf = vec![0u8; 64 << 10];
        while let Ok(n) = v2.read(&mut buf) {
            if n == 0 || c2.write_all(&frame(&buf[..n])).is_err() {
                break;
            }
        }
        let _ = c2.shutdown(std::net::Shutdown::Both);
    });
    // The browser to the guest.
    while let Some((op, p)) = read_frame(&mut c) {
        match op {
            0x8 => break,
            0x9 => {
                let mut pong = vec![0x8a, p.len().min(125) as u8];
                pong.extend_from_slice(&p[..p.len().min(125)]);
                let _ = c.write_all(&pong);
            }
            0x0..=0x2 if v.write_all(&p).is_err() => break,
            _ => {}
        }
    }
    let _ = v.shutdown(std::net::Shutdown::Both);
    let _ = down.join();
}

/// A file of the noVNC copy under `root`: its type and its bytes, or `None`
/// for a path that leaves the copy or names no file.
fn static_file(root: &std::path::Path, rest: &str) -> Option<(&'static str, Vec<u8>)> {
    let rest = rest.split(['?', '#']).next()?;
    let safe = !rest.is_empty()
        && rest
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != ".." && !c.contains('\\'));
    if !safe {
        return None;
    }
    let kind = match rest.rsplit('.').next()? {
        "js" => "text/javascript",
        "css" => "text/css",
        "html" => "text/html",
        "json" => "application/json",
        _ => "application/octet-stream",
    };
    std::fs::read(root.join(rest)).ok().map(|b| (kind, b))
}

/// The noVNC copy of the store, fetched where it is not there; a failure
/// keeps the page of the address only.
fn ensure_novnc() {
    let root = podvm_windows::tools::novnc_root();
    if root.join("core/rfb.js").is_file() {
        return;
    }
    if let Err(why) = fetch_novnc(&root) {
        eprintln!(
            "podvm windows view: no noVNC client in the store ({why}); the page names the \
             address of the bridge"
        );
    }
}

/// Fetch, check and unpack the npm package of noVNC into `root`: the
/// files under `package/`, regular files only, with no `..`.
fn fetch_novnc(root: &std::path::Path) -> Result<(), String> {
    let t = podvm_windows::tools::NOVNC;
    let part = root.with_extension("part");
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir_all(&part).map_err(|e| e.to_string())?;
    let tgz = part.join(t.name);
    let mut origin = super::super::origin::Http {
        url: t.url.to_string(),
    };
    let ceiling = podvm_windows::Ceiling {
        max_bytes: t.size,
        fsize: u64::MAX,
    };
    podvm_windows::fetch::store(&mut origin, &tgz, Some(t.sha256), ceiling, 3)
        .map_err(|e| e.to_string())?;
    let f = std::fs::File::open(&tgz).map_err(|e| e.to_string())?;
    let mut a = tar::Archive::new(flate2::read::GzDecoder::new(f));
    for e in a.entries().map_err(|e| e.to_string())? {
        let mut e = e.map_err(|e| e.to_string())?;
        if !e.header().entry_type().is_file() {
            continue;
        }
        let p = e.path().map_err(|e| e.to_string())?.into_owned();
        let Ok(rest) = p.strip_prefix("package") else {
            continue;
        };
        if rest
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            continue;
        }
        let dest = part.join(rest);
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        e.unpack(&dest).map_err(|e| e.to_string())?;
    }
    let _ = std::fs::remove_file(&tgz);
    let _ = std::fs::remove_dir_all(root);
    std::fs::rename(&part, root).map_err(|e| e.to_string())
}

/// `podvm windows view NAME [--port P]`.
pub(crate) fn view(args: &[String]) -> i32 {
    let mut name = None;
    let mut port: u16 = 0;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--port" => match it.next().and_then(|p| p.parse().ok()) {
                Some(p) => port = p,
                None => {
                    eprintln!("podvm windows view: --port takes a port");
                    return EXIT_FLAG_ERROR;
                }
            },
            n if name.is_none() && !n.starts_with('-') => name = Some(n.to_string()),
            other => {
                eprintln!("usage: podvm windows view NAME [--port P] ({other:?} is none of them)");
                return EXIT_FLAG_ERROR;
            }
        }
    }
    let Some(name) = name else {
        eprintln!("usage: podvm windows view NAME [--port P]");
        return EXIT_FLAG_ERROR;
    };
    if !find(&name).is_some_and(|r| r.running()) {
        eprintln!("podvm windows view: no running machine {name:?} (`podvm ps` lists them)");
        return EXIT_RUNTIME_ERROR;
    }
    ensure_novnc();
    let root = machines::root();
    let vnc = machines::vnc_socket(&root, &name);
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("podvm windows view: 127.0.0.1:{port}: {e}");
            return EXIT_RUNTIME_ERROR;
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
    println!("http://127.0.0.1:{port}/");
    let _ = std::io::stdout().flush();
    // The viewer ends with the machine.
    let watched = name.clone();
    std::thread::spawn(move || {
        while find(&watched).is_some_and(|r| r.running()) {
            std::thread::sleep(Duration::from_secs(1));
        }
        eprintln!("podvm windows view: the machine {watched} ended");
        std::process::exit(0);
    });
    for c in listener.incoming().flatten() {
        let (vnc, name) = (vnc.clone(), name.clone());
        std::thread::spawn(move || serve(c, &vnc, port, &name));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of the copy is served by its path; a path with `..` or an
    /// empty part is not.
    #[test]
    fn viewer_static_files_stay_in_the_copy() {
        let d = std::env::temp_dir().join(format!("podvm-novnc-{}", std::process::id()));
        std::fs::create_dir_all(d.join("core")).unwrap();
        std::fs::write(d.join("core/rfb.js"), "export default 1;").unwrap();
        assert_eq!(
            static_file(&d, "core/rfb.js?x=1"),
            Some(("text/javascript", b"export default 1;".to_vec()))
        );
        for bad in [
            "../etc/passwd",
            "core/../../x",
            "",
            "core//rfb.js",
            "/etc/passwd",
        ] {
            assert_eq!(static_file(&d, bad), None, "{bad}");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The example of RFC 6455, section 1.3.
    #[test]
    fn viewer_accept_key_is_the_one_of_rfc_6455() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    /// A masked frame of a client reads back, and a server frame carries
    /// each length form.
    #[test]
    fn viewer_frames_round_trip() {
        let mask = [1u8, 2, 3, 4];
        let data = b"RFB 003.008\n";
        let mut f = vec![0x82, 0x80 | data.len() as u8];
        f.extend_from_slice(&mask);
        f.extend(data.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        assert_eq!(read_frame(&mut &f[..]), Some((2, data.to_vec())));
        let unmasked = [0x82u8, 0x01, b'x'];
        assert_eq!(read_frame(&mut &unmasked[..]), None);
        assert_eq!(frame(b"ab"), vec![0x82, 2, b'a', b'b']);
        assert_eq!(&frame(&[0; 200])[..4], &[0x82, 126, 0, 200]);
        assert_eq!(&frame(&vec![0; 70_000])[..2], &[0x82, 127]);
    }
}
