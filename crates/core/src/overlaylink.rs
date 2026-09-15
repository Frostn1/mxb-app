//! The local link between MXB App and MXB Coach, so their overlays share one hotkey and hand
//! the screen to each other.
//!
//! Each app writes `<data dir>/overlay/<manager|coach>.json` (`pid, port, token, proto,
//! version`) and listens on `127.0.0.1`. The second to start reads the other's file and dials.
//! One TCP connection, newline-delimited JSON; the first line must carry the listener's token.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Bumped when a message changes meaning. Apps on different protos don't link.
pub const PROTO: u32 = 1;

/// How long a handshake may take before the connection is dropped.
const HANDSHAKE: Duration = Duration::from_secs(2);
/// How long to wait for a peer's port to answer. Loopback answers at once or not at all.
const CONNECT: Duration = Duration::from_millis(500);
/// No message is anywhere near this; a longer line is not from our peer.
const MAX_LINE: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum App {
    Manager,
    Coach,
}

impl App {
    pub fn other(self) -> App {
        match self {
            App::Manager => App::Coach,
            App::Coach => App::Manager,
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            App::Manager => "manager.json",
            App::Coach => "coach.json",
        }
    }
}

/// What an app writes about itself so the other can find it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Presence {
    pub pid: u32,
    pub port: u16,
    pub token: String,
    pub proto: u32,
    pub version: String,
}

/// `left, top, right, bottom`, physical pixels.
pub type Rect = (i32, i32, i32, i32);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Msg {
    /// First line each way. The dialer's carries the listener's token.
    Hello {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        token: String,
        app: App,
        proto: u32,
        #[serde(default)]
        version: String,
        #[serde(default)]
        pid: u32,
        #[serde(default)]
        tabs: Vec<String>,
    },
    /// Show your overlay, on `tab` (or where it was), over `rect` (or over the game).
    Show {
        #[serde(default)]
        tab: Option<String>,
        #[serde(default)]
        rect: Option<Rect>,
    },
    /// Put your overlay away.
    Hide,
    /// My overlay went away.
    Hidden {
        #[serde(default)]
        reason: String,
    },
    /// My overlay is up now, so put yours away.
    Shown,
    /// Coach → MXB App: I've let go of the key (or the setting changed), bind it.
    /// MXB App → Coach: bound, with the error if it failed.
    Rebind {
        #[serde(default)]
        error: Option<String>,
    },
}

/// The app on the other end.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo {
    pub app: App,
    pub pid: u32,
    pub version: String,
    pub tabs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Up(PeerInfo),
    Down,
    Msg(Msg),
}

type Handler = dyn Fn(&Link, Event) + Send + Sync;

#[derive(Clone)]
pub struct Link(Arc<Shared>);

struct Shared {
    me: App,
    dir: PathBuf,
    token: String,
    version: String,
    tabs: Vec<String>,
    conn: Mutex<Option<Conn>>,
    next_id: AtomicU64,
    stopped: AtomicBool,
    handler: Box<Handler>,
}

struct Conn {
    id: u64,
    /// Which app dialled. When both dial at once, both ends keep the one Coach dialled.
    dialer: App,
    info: PeerInfo,
    stream: TcpStream,
}

fn presence_path(dir: &Path, app: App) -> PathBuf {
    dir.join(app.file_name())
}

/// The other app's file, if it wrote one. Says nothing about whether it still runs.
pub fn read_presence(dir: &Path, app: App) -> Option<Presence> {
    serde_json::from_slice(&std::fs::read(presence_path(dir, app)).ok()?).ok()
}

/// Is anything listening where this file says? A refused port is an app that has gone.
pub fn alive(p: &Presence) -> bool {
    TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], p.port)), CONNECT).is_ok()
}

fn write_presence(dir: &Path, app: App, p: &Presence) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = presence_path(dir, app);
    // Written aside and moved in, so the other app never reads half a file.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(p)?)?;
    std::fs::rename(&tmp, &path)
}

fn new_token() -> String {
    let mut bytes = [0u8; 16];
    let _ = getrandom::getrandom(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn send_on(mut stream: &TcpStream, msg: &Msg) -> bool {
    let Ok(mut line) = serde_json::to_vec(msg) else {
        return false;
    };
    line.push(b'\n');
    stream.write_all(&line).is_ok()
}

/// One line, capped. `None` at the end of the stream or on a line that isn't a message.
fn read_line(reader: &mut BufReader<TcpStream>) -> Option<Option<Msg>> {
    let mut line = String::new();
    match reader.by_ref().take(MAX_LINE).read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(serde_json::from_str(line.trim()).ok()),
    }
}

impl Link {
    /// Listen, write the presence file, and hand every event to `handler`.
    pub fn start(
        me: App,
        dir: PathBuf,
        version: &str,
        tabs: &[&str],
        handler: impl Fn(&Link, Event) + Send + Sync + 'static,
    ) -> std::io::Result<Link> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let link = Link(Arc::new(Shared {
            me,
            dir,
            token: new_token(),
            version: version.to_string(),
            tabs: tabs.iter().map(|t| t.to_string()).collect(),
            conn: Mutex::new(None),
            next_id: AtomicU64::new(1),
            stopped: AtomicBool::new(false),
            handler: Box::new(handler),
        }));
        let presence = Presence {
            pid: std::process::id(),
            port,
            token: link.0.token.clone(),
            proto: PROTO,
            version: link.0.version.clone(),
        };
        write_presence(&link.0.dir, me, &presence)?;
        let accepting = link.clone();
        std::thread::Builder::new().name("overlay-link".into()).spawn(move || {
            for stream in listener.incoming() {
                if accepting.0.stopped.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let link = accepting.clone();
                std::thread::spawn(move || link.accept(stream));
            }
        })?;
        Ok(link)
    }

    pub fn me(&self) -> App {
        self.0.me
    }

    /// The linked app, if there is one.
    pub fn peer(&self) -> Option<PeerInfo> {
        self.0.conn.lock().ok()?.as_ref().map(|c| c.info.clone())
    }

    /// The other app's presence file, when it names a live app on a proto we can't speak.
    pub fn mismatch(&self) -> Option<Presence> {
        let p = read_presence(&self.0.dir, self.0.me.other())?;
        (p.proto != PROTO && alive(&p)).then_some(p)
    }

    /// Send to the linked app. False when there is none or the write failed.
    pub fn send(&self, msg: &Msg) -> bool {
        let Ok(slot) = self.0.conn.lock() else {
            return false;
        };
        slot.as_ref().is_some_and(|c| send_on(&c.stream, msg))
    }

    /// Delete the presence file and drop the link. Called on exit.
    pub fn stop(&self) {
        self.0.stopped.store(true, Ordering::SeqCst);
        let _ = std::fs::remove_file(presence_path(&self.0.dir, self.0.me));
        if let Ok(mut slot) = self.0.conn.lock() {
            if let Some(c) = slot.take() {
                let _ = c.stream.shutdown(Shutdown::Both);
            }
        }
    }

    fn hello(&self, token: String) -> Msg {
        Msg::Hello {
            token,
            app: self.0.me,
            proto: PROTO,
            version: self.0.version.clone(),
            pid: std::process::id(),
            tabs: self.0.tabs.clone(),
        }
    }

    /// Connect to the other app if its file names a live one on our proto. True when linked.
    pub fn dial(&self) -> bool {
        if self.peer().is_some() {
            return true;
        }
        let Some(p) = read_presence(&self.0.dir, self.0.me.other()) else {
            return false;
        };
        if p.proto != PROTO {
            return false;
        }
        let addr = SocketAddr::from(([127, 0, 0, 1], p.port));
        let Ok(stream) = TcpStream::connect_timeout(&addr, CONNECT) else {
            return false;
        };
        let _ = stream.set_read_timeout(Some(HANDSHAKE));
        let Ok(read_half) = stream.try_clone() else {
            return false;
        };
        if !send_on(&stream, &self.hello(p.token)) {
            return false;
        }
        let mut reader = BufReader::new(read_half);
        match read_line(&mut reader) {
            Some(Some(Msg::Hello { app, proto, version, pid, tabs, .. }))
                if app == self.0.me.other() && proto == PROTO =>
            {
                let info = PeerInfo { app, pid, version, tabs };
                self.establish(stream, reader, self.0.me, info)
            }
            _ => false,
        }
    }

    fn accept(&self, stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(HANDSHAKE));
        let Ok(read_half) = stream.try_clone() else { return };
        let mut reader = BufReader::new(read_half);
        let Some(Some(Msg::Hello { token, app, proto, version, pid, tabs })) = read_line(&mut reader)
        else {
            return;
        };
        if !same_token(&token, &self.0.token) || app != self.0.me.other() {
            log::warn!("overlay link: refused a connection that didn't carry our token");
            return;
        }
        // Answered either way, so a peer on another proto learns ours.
        if !send_on(&stream, &self.hello(String::new())) || proto != PROTO {
            return;
        }
        self.establish(stream, reader, app, PeerInfo { app, pid, version, tabs });
    }

    fn establish(&self, stream: TcpStream, reader: BufReader<TcpStream>, dialer: App, info: PeerInfo) -> bool {
        let _ = stream.set_read_timeout(None);
        let id = self.0.next_id.fetch_add(1, Ordering::SeqCst);
        {
            let Ok(mut slot) = self.0.conn.lock() else {
                return false;
            };
            if let Some(cur) = slot.as_ref() {
                if cur.dialer == App::Coach || dialer != App::Coach {
                    let _ = stream.shutdown(Shutdown::Both);
                    return true;
                }
                let _ = cur.stream.shutdown(Shutdown::Both);
            }
            let Ok(keep) = stream.try_clone() else {
                return false;
            };
            *slot = Some(Conn { id, dialer, info: info.clone(), stream: keep });
        }
        log::info!("overlay link up with {:?} v{}", info.app, info.version);
        (self.0.handler)(self, Event::Up(info));
        let link = self.clone();
        std::thread::spawn(move || link.read_loop(id, reader));
        true
    }

    fn read_loop(&self, id: u64, mut reader: BufReader<TcpStream>) {
        while let Some(msg) = read_line(&mut reader) {
            match msg {
                Some(Msg::Hello { .. }) | None => {}
                Some(msg) => (self.0.handler)(self, Event::Msg(msg)),
            }
        }
        let dropped = match self.0.conn.lock() {
            Ok(mut slot) if slot.as_ref().is_some_and(|c| c.id == id) => {
                slot.take();
                true
            }
            _ => false,
        };
        if dropped {
            log::info!("overlay link down");
            (self.0.handler)(self, Event::Down);
        }
    }
}

/// Compared in full whatever the first difference, so timing says nothing about the token.
fn same_token(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("overlaylink-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn start(app: App, dir: &Path) -> (Link, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let tabs: &[&str] = match app {
            App::Manager => &["presets"],
            App::Coach => &["tips", "hud"],
        };
        let link = Link::start(app, dir.to_path_buf(), "1.0.0", tabs, move |_, e| {
            let _ = tx.lock().unwrap().send(e);
        })
        .unwrap();
        (link, rx)
    }

    fn next(rx: &mpsc::Receiver<Event>) -> Event {
        rx.recv_timeout(Duration::from_secs(5)).expect("an event")
    }

    #[test]
    fn two_links_in_one_process_talk() {
        let dir = temp_dir("talk");
        let (manager, mrx) = start(App::Manager, &dir);
        let (coach, crx) = start(App::Coach, &dir);
        assert!(coach.dial(), "coach finds the manager by its file");

        match next(&crx) {
            Event::Up(p) => assert_eq!((p.app, p.tabs), (App::Manager, vec!["presets".to_string()])),
            e => panic!("expected up, got {e:?}"),
        }
        match next(&mrx) {
            Event::Up(p) => assert_eq!(p.tabs, vec!["tips".to_string(), "hud".to_string()]),
            e => panic!("expected up, got {e:?}"),
        }

        let show = Msg::Show { tab: Some("hud".into()), rect: Some((1, 2, 3, 4)) };
        assert!(manager.send(&show));
        assert_eq!(next(&crx), Event::Msg(show));
        assert!(coach.send(&Msg::Rebind { error: None }));
        assert_eq!(next(&mrx), Event::Msg(Msg::Rebind { error: None }));

        manager.stop();
        assert_eq!(next(&crx), Event::Down, "the other end hears the link drop");
        assert!(read_presence(&dir, App::Manager).is_none(), "stop deletes the file");
        coach.stop();
    }

    #[test]
    fn a_wrong_token_is_rejected() {
        let dir = temp_dir("token");
        let (manager, mrx) = start(App::Manager, &dir);
        let p = read_presence(&dir, App::Manager).unwrap();
        let stream = TcpStream::connect(("127.0.0.1", p.port)).unwrap();
        let hello = Msg::Hello {
            token: "not-it".into(),
            app: App::Coach,
            proto: PROTO,
            version: String::new(),
            pid: 1,
            tabs: vec![],
        };
        assert!(send_on(&stream, &hello));
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut buf = String::new();
        let n = BufReader::new(stream).read_line(&mut buf).unwrap_or(0);
        assert_eq!(n, 0, "closed without an answer: {buf}");
        assert!(manager.peer().is_none());
        assert!(mrx.try_recv().is_err(), "no link came up");
        manager.stop();
    }

    #[test]
    fn a_stale_file_is_ignored() {
        let dir = temp_dir("stale");
        // A port nothing listens on: bind one, note it, let it go.
        let port = TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
        let stale = Presence { pid: 999_999, port, token: "x".into(), proto: PROTO, version: "0".into() };
        write_presence(&dir, App::Manager, &stale).unwrap();
        assert!(!alive(&stale));

        let (coach, crx) = start(App::Coach, &dir);
        assert!(!coach.dial(), "a refused port counts as absent");
        assert!(coach.peer().is_none());
        assert!(coach.mismatch().is_none());
        assert!(crx.try_recv().is_err());
        coach.stop();
    }

    #[test]
    fn a_peer_on_another_proto_is_a_mismatch_not_a_link() {
        let dir = temp_dir("proto");
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let old = Presence { pid: 1, port, token: "x".into(), proto: PROTO + 1, version: "9".into() };
        write_presence(&dir, App::Manager, &old).unwrap();
        let (coach, _crx) = start(App::Coach, &dir);
        assert!(!coach.dial());
        assert_eq!(coach.mismatch().map(|p| p.proto), Some(PROTO + 1));
        coach.stop();
    }

    #[test]
    fn messages_round_trip_as_one_line_each() {
        let msgs = [
            Msg::Hide,
            Msg::Shown,
            Msg::Hidden { reason: "hotkey".into() },
            Msg::Show { tab: None, rect: None },
            Msg::Rebind { error: Some("taken".into()) },
        ];
        for m in msgs {
            let text = serde_json::to_string(&m).unwrap();
            assert!(!text.contains('\n'));
            assert_eq!(serde_json::from_str::<Msg>(&text).unwrap(), m);
        }
        assert_eq!(serde_json::to_string(&Msg::Hide).unwrap(), r#"{"t":"hide"}"#);
    }
}
