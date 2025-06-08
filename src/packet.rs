use nex_packet::{icmp::IcmpType, icmpv6::Icmpv6Type, ipv4::Ipv4Packet, ipv6::Ipv6Packet, Packet};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6}, time::Duration};

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

type ParseResult = Option<(IpAddr, usize, bool)>

fn parse_icmpv6(packet: Ipv6Packet, target: Ipv6Addr) -> ParseResult {
    let src = packet.get_source();
    if let Some(icmp_packet) = nex_packet::icmp::IcmpPacket::new(packet.payload()) {
        return match icmp_packet.get_icmp_type() {
            nex_packet::icmp::IcmpType::EchoReply => {
                let packet = icmp_packet.packet();
                if src == target {
                    let seq_number = u16::from_be_bytes([packet[6], packet[7]]);
                    let hop: usize = seq_number.into();
                    return Some((src.into(), hop, true));
                } else {
                    None
                }
            },
            nex_packet::icmp::IcmpType::TimeExceeded => {
                let packet = icmp_packet.packet();
                let seq_number = u16::from_be_bytes([packet[34], packet[35]]);
                let hop: usize = seq_number.into();
                return Some((src.into(), hop, false));
            },
            _ => None
        };
    }

    None

}

fn parse_icmpv4(packet: Ipv4Packet, target: Ipv4Addr) -> ParseResult {
    let src = packet.get_source();
    if let Some(icmp_packet) = nex_packet::icmp::IcmpPacket::new(packet.payload()) {
        return match icmp_packet.get_icmp_type() {
            nex_packet::icmp::IcmpType::EchoReply => {
                let packet = icmp_packet.packet();
                if src == target {
                    let seq_number = u16::from_be_bytes([packet[6], packet[7]]);
                    let hop: usize = seq_number.into();
                    return Some((src.into(), hop, true));
                } else {
                    None
                }
            },
            nex_packet::icmp::IcmpType::TimeExceeded => {
                let packet = icmp_packet.packet();
                let seq_number = u16::from_be_bytes([packet[34], packet[35]]);
                let hop: usize = seq_number.into();
                return Some((src.into(), hop, false));
            },
            _ => None
        };
    }

    None
}

pub fn parse_ipv6(packet: Ipv6Packet, target: Ipv6Addr, mode: PingMode) -> ParseResult {
    match mode {
        PingMode::UDP => todo!(),
        PingMode::TCP => todo!(),
        PingMode::ICMP => parse_icmpv6(packet, target),
    }
}

pub fn parse_ipv4(packet: Ipv4Packet, target: Ipv4Addr, mode: PingMode) -> ParseResult {
    match mode {
        PingMode::UDP => todo!(),
        PingMode::TCP => todo!(),
        PingMode::ICMP => parse_icmpv4(packet, target),
    }
}

pub fn parse_packet(buf: &[u8], target: IpAddr, mode: PingMode) -> ParseResult {
    match target {
        IpAddr::V4(target) => {
            if let Some(packet) = nex_packet::ipv4::Ipv4Packet::new(buf) {
                return parse_ipv4(packet, target, mode);
            }
        },
        IpAddr::V6(target) => {
            if let Some(packet) = nex_packet::ipv6::Ipv6Packet::new(buf) {
                return parse_ipv6(packet, target, mode);
            }
        },
    };

    None
}

pub async fn send_probe(ip: IpAddr, ttl: usize, timeout: Duration, mode: PingMode) {
    let seq_number = ttl as u16;

    match mode {
        PingMode::ICMP => {
            let socket = match ip {
                IpAddr::V4(_) => Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4)),
                IpAddr::V6(_) => Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6)),
            }.expect("failed to make raw socket!");

            let sock_addr: SockAddr = match ip {
                IpAddr::V4(ip) => SocketAddrV4::new(ip, 0).into(),
                IpAddr::V6(ip) => SocketAddrV6::new(ip, 0, 0, 0).into(),
            };

            socket.set_write_timeout(Some(timeout)).expect("failed to set write timeout!");
            socket.set_ttl(ttl as u32).expect("failed to set ttl!");

            let packet = match ip {
                IpAddr::V4(_) => build_icmpv4_echo_packet(seq_number),
                IpAddr::V6(_) => build_icmpv6_echo_packet(seq_number),
            };

            socket.send_to(&packet, &sock_addr).expect("failed to send probe!");
        }

        //PingMode::UDP => {
        //    let identifier_port: u16 = 33433 + ttl as u16;
        //    unimplemented!()
        //},
        _ => unimplemented!()
    }
}

