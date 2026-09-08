//! Ask a public server what this socket looks like from outside.
//!
//! Two riders on home connections both sit behind a router doing NAT. Neither knows its own
//! public address, and neither can be reached at the private one it does know — so without
//! this, they can describe themselves to each other perfectly and still never connect.
//!
//! A STUN binding request is the whole answer: send eight bytes of header to a public server
//! *from the socket voice will actually use*, and it replies with the address and port your
//! router put on the packet. That address, offered as a candidate, is what the other rider
//! aims at, and the router — having just seen an outbound packet to that server — has a
//! mapping open for the reply to come back through.
//!
//! Implemented here rather than pulled in: it is one request and one attribute of one reply,
//! and voice already owns the socket it has to be sent from.

use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// Binding request, the only method we use.
const BINDING_REQUEST: u16 = 0x0001;
const BINDING_RESPONSE: u16 = 0x0101;

/// Present in every STUN message since RFC 5389; also what tells a reply apart from the
/// game's own traffic if anything else ever shares this socket.
const MAGIC_COOKIE: u32 = 0x2112_A442;

/// The attribute we came for: our address as the server saw it, XOR-ed with the cookie so
/// that middleboxes rewriting bare addresses in payloads don't silently corrupt it.
const XOR_MAPPED_ADDRESS: u16 = 0x0020;

const HEADER_BYTES: usize = 20;

/// How long to wait for one server before trying the next.
const REPLY_TIMEOUT: Duration = Duration::from_millis(1200);

/// Build a binding request with the given transaction id.
fn request(transaction: &[u8; 12]) -> [u8; HEADER_BYTES] {
    let mut msg = [0u8; HEADER_BYTES];
    msg[0..2].copy_from_slice(&BINDING_REQUEST.to_be_bytes());
    // Length: no attributes.
    msg[2..4].copy_from_slice(&0u16.to_be_bytes());
    msg[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
    msg[8..20].copy_from_slice(transaction);
    msg
}

/// Pull our address out of a binding response, if this is one and it matches.
pub fn parse_response(bytes: &[u8], transaction: &[u8; 12]) -> Option<SocketAddr> {
    if bytes.len() < HEADER_BYTES {
        return None;
    }
    if u16::from_be_bytes([bytes[0], bytes[1]]) != BINDING_RESPONSE {
        return None;
    }
    if u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) != MAGIC_COOKIE {
        return None;
    }
    if &bytes[8..20] != transaction {
        return None;
    }

    let declared = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
    let end = HEADER_BYTES.checked_add(declared)?.min(bytes.len());
    let mut at = HEADER_BYTES;
    while at + 4 <= end {
        let kind = u16::from_be_bytes([bytes[at], bytes[at + 1]]);
        let len = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        let value_at = at + 4;
        let value_end = value_at.checked_add(len)?;
        if value_end > end {
            return None;
        }
        if kind == XOR_MAPPED_ADDRESS {
            return parse_xor_mapped(&bytes[value_at..value_end]);
        }
        // Attributes are padded to a multiple of four.
        at = value_end + ((4 - (len % 4)) % 4);
    }
    None
}

fn parse_xor_mapped(value: &[u8]) -> Option<SocketAddr> {
    // family(1) after a reserved byte, then port, then address.
    if value.len() < 8 || value[1] != 0x01 {
        // 0x02 is IPv6. Left alone deliberately: the candidate we want is the one the other
        // rider can reach, and a v6 address is only useful if both ends have v6.
        return None;
    }
    let port = u16::from_be_bytes([value[2], value[3]]) ^ (MAGIC_COOKIE >> 16) as u16;
    let cookie = MAGIC_COOKIE.to_be_bytes();
    let ip = std::net::Ipv4Addr::new(
        value[4] ^ cookie[0],
        value[5] ^ cookie[1],
        value[6] ^ cookie[2],
        value[7] ^ cookie[3],
    );
    Some(SocketAddr::from((ip, port)))
}

/// Discover this socket's public address, trying each server until one answers.
///
/// The socket must be the one voice will send from — the answer is only true for that
/// socket's mapping, and asking from a different one would produce an address the other
/// rider's packets can never arrive at.
///
/// `None` means every server was unreachable. That is survivable and not fatal: riders on
/// the same network still connect on their local addresses, and a room with an unreachable
/// peer must degrade to that peer being silent, never to voice failing.
pub fn public_address(socket: &UdpSocket, servers: &[SocketAddr]) -> Option<SocketAddr> {
    let previous = socket.read_timeout().ok().flatten();
    let _ = socket.set_read_timeout(Some(REPLY_TIMEOUT));
    let restore = |socket: &UdpSocket| {
        let _ = socket.set_read_timeout(previous);
    };

    let mut buf = [0u8; 1024];
    for server in servers {
        // A fresh id per attempt, so a late reply from the previous server can't be mistaken
        // for this one's.
        let transaction = transaction_id();
        if socket.send_to(&request(&transaction), server).is_err() {
            continue;
        }
        let deadline = Instant::now() + REPLY_TIMEOUT;
        while Instant::now() < deadline {
            let Ok((n, from)) = socket.recv_from(&mut buf) else { break };
            if from != *server {
                // Something else on this socket. Not ours to interpret.
                continue;
            }
            if let Some(address) = parse_response(&buf[..n], &transaction) {
                restore(socket);
                return Some(address);
            }
        }
    }
    restore(socket);
    None
}

/// Twelve bytes that will not repeat. Not a secret — it only has to distinguish one
/// in-flight request from another — so the process id and the clock are plenty.
fn transaction_id() -> [u8; 12] {
    let mut id = [0u8; 12];
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    id[0..8].copy_from_slice(&now.to_le_bytes());
    id[8..12].copy_from_slice(&std::process::id().to_le_bytes());
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A binding response carrying `address`, built the way a server would.
    fn response(transaction: &[u8; 12], address: SocketAddr) -> Vec<u8> {
        let SocketAddr::V4(v4) = address else { panic!("v4 only") };
        let cookie = MAGIC_COOKIE.to_be_bytes();
        let octets = v4.ip().octets();
        let mut value = vec![0x00, 0x01];
        value.extend_from_slice(&(v4.port() ^ (MAGIC_COOKIE >> 16) as u16).to_be_bytes());
        for i in 0..4 {
            value.push(octets[i] ^ cookie[i]);
        }

        let mut msg = Vec::new();
        msg.extend_from_slice(&BINDING_RESPONSE.to_be_bytes());
        msg.extend_from_slice(&((4 + value.len()) as u16).to_be_bytes());
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        msg.extend_from_slice(transaction);
        msg.extend_from_slice(&XOR_MAPPED_ADDRESS.to_be_bytes());
        msg.extend_from_slice(&(value.len() as u16).to_be_bytes());
        msg.extend_from_slice(&value);
        msg
    }

    #[test]
    fn reads_back_the_address_a_server_reports() {
        let id = transaction_id();
        let address: SocketAddr = "203.0.113.7:51234".parse().unwrap();
        assert_eq!(parse_response(&response(&id, address), &id), Some(address));
    }

    #[test]
    fn ignores_a_reply_to_someone_elses_request() {
        let mine = transaction_id();
        let mut theirs = mine;
        theirs[0] ^= 0xff;
        let bytes = response(&theirs, "203.0.113.7:51234".parse().unwrap());
        assert_eq!(parse_response(&bytes, &mine), None);
    }

    #[test]
    fn ignores_anything_that_isnt_a_binding_response() {
        let id = transaction_id();
        assert_eq!(parse_response(&[], &id), None);
        assert_eq!(parse_response(&request(&id), &id), None, "our own request is not a reply");
        let mut no_cookie = response(&id, "203.0.113.7:1".parse().unwrap());
        no_cookie[4] ^= 0xff;
        assert_eq!(parse_response(&no_cookie, &id), None);
    }

    #[test]
    fn a_truncated_reply_never_panics() {
        let id = transaction_id();
        let bytes = response(&id, "203.0.113.7:51234".parse().unwrap());
        for n in 0..bytes.len() {
            let _ = parse_response(&bytes[..n], &id);
        }
    }

    #[test]
    fn a_lying_attribute_length_never_panics() {
        let id = transaction_id();
        let mut bytes = response(&id, "203.0.113.7:51234".parse().unwrap());
        // Claim the attribute is far longer than the message.
        bytes[HEADER_BYTES + 2] = 0xff;
        bytes[HEADER_BYTES + 3] = 0xff;
        assert_eq!(parse_response(&bytes, &id), None);
    }

    /// Live, and deliberately so: the format is easy to get subtly wrong in a way only a
    /// real server notices. Ignored by default so the suite doesn't need the network.
    #[test]
    #[ignore = "needs the internet"]
    fn finds_our_address_through_a_real_stun_server() {
        let socket = UdpSocket::bind("0.0.0.0:0").expect("bind");
        let servers: Vec<SocketAddr> = ["stun.cloudflare.com:3478", "stun.l.google.com:19302"]
            .iter()
            .filter_map(|s| std::net::ToSocketAddrs::to_socket_addrs(s).ok()?.find(|a| a.is_ipv4()))
            .collect();
        let found = public_address(&socket, &servers).expect("a public address");
        assert!(!found.ip().is_unspecified());
        println!("this machine looks like {found} from outside");
    }
}
