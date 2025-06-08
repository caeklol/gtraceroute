use nex_packet::{icmp::IcmpType, icmpv6::Icmpv6Type, ipv4::Ipv4Packet, ipv6::Ipv6Packet, Packet};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::net::UdpSocket;
use std::{io::Error, net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6}, time::Duration};

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

type ParseResult = Option<(IpAddr, usize, bool)>;

pub fn parse_ipv6(packet: Ipv6Packet, target: Ipv6Addr, mode: PingMode) -> ParseResult {
    let src = packet.get_source();
    if let Some(icmp_packet) = nex_packet::icmp::IcmpPacket::new(packet.payload()) {
        return match icmp_packet.get_icmp_type() {
            nex_packet::icmp::IcmpType::EchoReply => {
                if mode != PingMode::ICMP { 
                    return None
                }

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
                let identifier: usize = match mode {
                    PingMode::UDP => u16::from_be_bytes([packet[54], packet[55]]).into(),
                    PingMode::TCP => todo!(),
                    PingMode::ICMP => u16::from_be_bytes([packet[54], packet[55]]).into(),
                };

                return Some((src.into(), identifier, false));
            },
            _ => None
        };
    }

    None
}

pub fn parse_ipv4(packet: Ipv4Packet, target: Ipv4Addr, mode: PingMode) -> ParseResult {
    let src = packet.get_source();
    if let Some(icmp_packet) = nex_packet::icmp::IcmpPacket::new(packet.payload()) {
        return match icmp_packet.get_icmp_type() {
            nex_packet::icmp::IcmpType::EchoReply => {
                if mode != PingMode::ICMP { 
                    return None
                }

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
                let identifier: usize = match mode {
                    PingMode::UDP => (u16::from_be_bytes([packet[30], packet[31]]) - 33433).into(),
                    PingMode::TCP => todo!(),
                    PingMode::ICMP => u16::from_be_bytes([packet[34], packet[35]]).into(),
                };

                if mode == PingMode::UDP {
                    return None
                }

                return Some((src.into(), identifier, false));
            },
            _ => None
        };
    }

    None
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

pub async fn send_probe(ip: IpAddr, ttl: usize, timeout: Duration, mode: PingMode) -> Result<(), Error> {
    let seq_number = ttl as u16;
    
    match mode {
        PingMode::ICMP => {
            let socket = match ip {
                IpAddr::V4(_) => Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4)),
                IpAddr::V6(_) => Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6)),
            }?;

            socket.set_write_timeout(Some(timeout))?;
            socket.set_ttl(ttl as u32)?;

            let packet = match ip {
                IpAddr::V4(_) => build_icmpv4_echo_packet(seq_number),
                IpAddr::V6(_) => build_icmpv6_echo_packet(seq_number),
            };

            let sock_addr: SockAddr = match ip {
                IpAddr::V4(ip) => SocketAddrV4::new(ip, 0).into(),
                IpAddr::V6(ip) => SocketAddrV6::new(ip, 0, 0, 0).into(),
            };

            socket.send_to(&packet, &sock_addr)?;
            return Ok(());
        }

        PingMode::UDP => {
            let port = 33433 + seq_number;
            
            let socket = UdpSocket::bind("0.0.0.0:0").await?;

            println!(":3");
            socket.set_ttl(ttl as u32)?;
            socket.send_to(&[0; 10], (ip, port)).await?;

            return Ok(());
        },
        _ => unimplemented!()
    }
}

