use std::net::{SocketAddr, TcpStream, UdpSocket};

use anyhow::Error;

pub(crate) trait TcpHandler: Send + Sync {
	fn handler(&self, stream: TcpStream, _id: u32) -> Result<(), Error>;

	fn shutdown(&self) {}
}

pub(crate) trait UdpHandler: Send + Sync {
	fn handler(&self, data: &[u8], socket: &mut UdpSocket, addr: SocketAddr) -> Result<(), Error>;

	fn shutdown(&self) {}
}
