use nex_packet::{icmp::IcmpType, icmpv6::Icmpv6Type, Packet};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::net::UdpSocket;
use std::{io::Error, net::{IpAddr, SocketAddrV4, SocketAddrV6}, time::Duration};

use crate::tracer::PingMode;

const ICMPV4_HEADER_SIZE: usize =
    nex_packet::icmp::echo_request::MutableEchoRequestPacket::minimum_packet_size();
const ICMPV6_HEADER_SIZE: usize =
    nex_packet::icmpv6::echo_request::MutableEchoRequestPacket::minimum_packet_size();

pub fn build_icmpv4_packet(icmp_packet: &mut nex_packet::icmp::echo_request::MutableEchoRequestPacket, seq: u16) {
    icmp_packet.set_icmp_type(IcmpType::EchoRequest);
    icmp_packet.set_sequence_number(seq);
    icmp_packet.set_identifier(rand::random::<u16>());
    let icmp_check_sum = nex_packet::util::checksum(&icmp_packet.packet(), 1);
    icmp_packet.set_checksum(icmp_check_sum);
}

pub fn build_icmpv6_packet(icmp_packet: &mut nex_packet::icmpv6::echo_request::MutableEchoRequestPacket, seq: u16) {
    icmp_packet.set_icmpv6_type(Icmpv6Type::EchoRequest);
    icmp_packet.set_identifier(seq);
    icmp_packet.set_sequence_number(rand::random::<u16>());
    let icmp_check_sum = nex_packet::util::checksum(&icmp_packet.packet(), 1);
    icmp_packet.set_checksum(icmp_check_sum);
}

pub fn build_icmpv4_echo_packet(seq: u16) -> Vec<u8> {
    let mut buf = vec![0; ICMPV4_HEADER_SIZE];
    let mut icmp_packet =
        nex_packet::icmp::echo_request::MutableEchoRequestPacket::new(&mut buf[..]).unwrap();
    build_icmpv4_packet(&mut icmp_packet, seq);
    icmp_packet.packet().to_vec()
}

pub fn build_icmpv6_echo_packet(seq: u16) -> Vec<u8> {
    let mut buf = vec![0; ICMPV6_HEADER_SIZE];
    let mut icmp_packet =
        nex_packet::icmpv6::echo_request::MutableEchoRequestPacket::new(&mut buf[..]).unwrap();
    build_icmpv6_packet(&mut icmp_packet, seq);
    icmp_packet.packet().to_vec()
}

type ParseResult = Option<(IpAddr, usize, usize)>;

fn parse_ip(buf: &[u8], src: IpAddr, target: IpAddr, mode: PingMode, attempts: usize) -> ParseResult {
    if let Some(icmp_packet) = nex_packet::icmp::IcmpPacket::new(buf) {
        let identifier = match icmp_packet.get_icmp_type() {
            nex_packet::icmp::IcmpType::EchoReply => {
                if mode != PingMode::ICMP || src == target { 
                    return None
                }

                let packet = icmp_packet.packet();
                let seq_number = u16::from_be_bytes([packet[6], packet[7]]);
                let seq_number: usize = seq_number.into();
                seq_number - 1
            },
            nex_packet::icmp::IcmpType::DestinationUnreachable => {
                if mode != PingMode::UDP || src != target { 
                    return None
                }

                let packet = icmp_packet.packet();
                let seq_number = u16::from_be_bytes([packet[30], packet[31]]) - 33433;
                let seq_number: usize = seq_number.into();
                seq_number - 1
            },
            nex_packet::icmp::IcmpType::TimeExceeded => {
                let packet = icmp_packet.packet();
                let offset = match (target, mode) {
                    (IpAddr::V6(_), _) => [54, 55],
                    (IpAddr::V4(_), PingMode::UDP) => [30, 31],
                    (IpAddr::V4(_), PingMode::ICMP) => [34, 35],
                    (IpAddr::V4(_), PingMode::TCP) => return None, // or todo!()
                };

                let raw = u16::from_be_bytes([packet[offset[0]], packet[offset[1]]]);
                if mode == PingMode::UDP || mode == PingMode::TCP {
                    (raw - 33433).into()
                } else {
                    raw.into()
                }
            },
            _ => {
                return None.into();
            }
        };

        let ttl = identifier / attempts;
        let attempt = identifier % attempts;

        return Some((src.into(), ttl, attempt));
    } else {
        return None;
    }
}

pub fn parse_packet(buf: &[u8], target: IpAddr, mode: PingMode, attempts: usize) -> ParseResult {
    let (payload, source) = match target {
        IpAddr::V4(_) => {
            if let Some(packet) = nex_packet::ipv4::Ipv4Packet::new(buf) {
                let src = packet.get_source();
                let payload = packet.payload().to_owned();
                (payload, IpAddr::V4(src))
            } else {
                return None;
            }
        },
        IpAddr::V6(_) => {
            if let Some(packet) = nex_packet::ipv6::Ipv6Packet::new(buf) {
                let src = packet.get_source();
                let payload = packet.payload().to_owned();
                (payload, IpAddr::V6(src))
            } else {
                return None;
            }
        },
    };

    return parse_ip(&payload, source, target, mode, attempts);
}

fn gen_bytes(len: u8) -> Vec<u8> {
    let mut vec = Vec::new();
    for n in 0..len {
        vec.push(n+40);
    }
    return vec;
}

async fn send_probe_packet(ip: IpAddr, ttl: usize, id: usize, timeout: Duration, mode: PingMode) -> Result<(), Error> {
    let id: u16 = id.try_into().unwrap();

    match mode {
        PingMode::ICMP => {
            let socket = match ip {
                IpAddr::V4(_) => Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4)),
                IpAddr::V6(_) => Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6)),
            }?;

            socket.set_write_timeout(Some(timeout))?;
            socket.set_ttl(ttl as u32)?;

            let packet = match ip {
                IpAddr::V4(_) => build_icmpv4_echo_packet(id),
                IpAddr::V6(_) => build_icmpv6_echo_packet(id),
            };

            let sock_addr: SockAddr = match ip {
                IpAddr::V4(ip) => SocketAddrV4::new(ip, 0).into(),
                IpAddr::V6(ip) => SocketAddrV6::new(ip, 0, 0, 0).into(),
            };

            socket.send_to(&packet, &sock_addr)?;
            return Ok(());
        }

        PingMode::UDP => {
            let socket = UdpSocket::bind("0.0.0.0:0").await?;

            socket.set_ttl(ttl as u32)?;
            socket.send_to(&gen_bytes(32), (ip, id)).await?;

            return Ok(());
        },
        _ => unimplemented!()
    }
}


pub async fn send_probe(ip: IpAddr, hop: usize, timeout: Duration, mode: PingMode, attempts: usize) -> Vec<impl futures::Future<Output = Result<(), Error>>> {
    let mut vec = Vec::new();
    for n in 0..attempts {
        let mut id = (attempts * hop) + n;
        if mode == PingMode::UDP || mode == PingMode::TCP {
            id = id + 33433;
        }

        vec.push(send_probe_packet(ip, hop+1, id, timeout, mode));
    }

    vec
}

