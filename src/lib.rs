#![feature(associated_consts)]
#![feature(try_from)]

extern crate bytes;

use bytes::{Bytes, BytesMut, BufMut, LittleEndian};

use std::io;
use std::io::{Read, Write, Error, ErrorKind};
use std::convert::TryFrom;

enum EnttecError {
    Frame,
    Length,
    Label,
    Serial(Error),
}

trait EnttecWidget: Read + Write {
    fn send_packet<S, R>(&mut self, packet: S) -> Result<R, EnttecError>
        where S: EnttecPacket,
              R: EnttecPacket
    {
        let mut data = BytesMut::with_capacity(S::size() + 5);

        data.put_u8(0x7E);
        data.put_u8(S::LABEL);
        data.put_u16::<LittleEndian>(S::size() as u16);
        data.put_slice(&*packet.into());
        data.put_u8(0xE7);

        match self.write(&*data.freeze()) {
            Ok(count) => {
                if count == S::size() + 5 {
                    let mut data = vec![0; R::size()+5];
                    match self.read_exact(&mut *data) {
                        Ok(_) => {
                            match R::try_from(Bytes::from(data)) {
                                Ok(packet) => Ok(packet),
                                Err(e) => Err(e),
                            }
                        }
                        Err(err) => Err(EnttecError::Serial(err)),
                    }
                } else {
                    Err(EnttecError::Length)
                }
            }
            Err(err) => Err(EnttecError::Serial(err)),
        }
    }
}

struct ValidFrame(Bytes);

trait EnttecPacket: TryFrom<Bytes, Err = EnttecError> + Into<Bytes> {
    const LABEL: u8;

    fn frame_check(packet: Bytes) -> Result<ValidFrame, EnttecError> {
        if packet.len() != Self::size() + 5 {
            Err(EnttecError::Length)
        } else if packet[0] != 0x7E {
            Err(EnttecError::Frame)
        } else if packet[packet.len() - 1] != 0xE7 {
            Err(EnttecError::Frame)
        } else if packet[1] != Self::LABEL {
            Err(EnttecError::Label)
        } else {
            Ok(ValidFrame(packet))
        }
    }

    fn frame_data(packet: &ValidFrame) -> &[u8] {
        let &ValidFrame(ref bytes) = packet;
        &bytes[4..bytes.len() - 1]
    }

    fn size(&self) -> usize;
}

struct GetParameters {
    param_size: u16,
}
impl EnttecPacket for GetParameters {
    const LABEL: u8 = 3;
    fn size() -> usize {
        2
    }
}
impl TryFrom<Bytes> for GetParameters {
    type Err = EnttecError;
    fn try_from(value: Bytes) -> Result<GetParameters, EnttecError> {
        match Self::frame_check(value) {
            Ok(frame) => {
                let slice = Self::frame_data(&frame);
                Ok(GetParameters { param_size: (slice[0] as u16) << 8 | (slice[1] as u16) })
            }
            Err(e) => Err(e),
        }
    }
}
impl Into<Bytes> for GetParameters {
    fn into(self) -> Bytes {
        let mut bytes = BytesMut::with_capacity(2);
        bytes.put_u16::<LittleEndian>(self.param_size);
        bytes.freeze()
    }
}

#[derive(Debug)]
struct GetParametersReply {
    firm_lsb: u8,
    firm_msb: u8,
    dmx_break: u8,
    dmx_mab: u8,
    dmx_rate: u8,
    user_data: Vec<u8>,
}
impl EnttecPacket for GetParametersReply {
    const LABEL: u8 = 3;
    fn size() -> usize {
        5
    }
}
impl TryFrom<Bytes> for GetParametersReply {
    type Err = EnttecError;
    fn try_from(value: Bytes) -> Result<GetParametersReply, EnttecError> {
        match Self::frame_check(value) {
            Ok(frame) => {
                let slice = Self::frame_data(&frame);
                Ok(GetParametersReply {
                       firm_lsb: slice[0],
                       firm_msb: slice[1],
                       dmx_break: slice[2],
                       dmx_mab: slice[3],
                       dmx_rate: slice[4],
                   })
            }
            Err(e) => Err(e),
        }
    }
}
impl Into<Bytes> for GetParametersReply {
    fn into(self) -> Bytes {
        let mut bytes = BytesMut::with_capacity(Self::size());
        bytes.put_u8(self.firm_lsb);
        bytes.put_u8(self.firm_msb);
        bytes.put_u8(self.dmx_break);
        bytes.put_u8(self.dmx_mab);
        bytes.put_u8(self.dmx_rate);
        bytes.freeze()
    }
}

struct GetSerialNumber;
impl EnttecPacket for GetSerialNumber {
    const LABEL: u8 = 10;
    fn size() -> usize {
        0
    }
}
impl TryFrom<Bytes> for GetSerialNumber {
    type Err = EnttecError;
    fn try_from(value: Bytes) -> Result<GetSerialNumber, EnttecError> {
        match Self::frame_check(value) {
            Ok(_) => Ok(GetSerialNumber {}),
            Err(e) => Err(e),
        }
    }
}
impl Into<Bytes> for GetSerialNumber {
    fn into(self) -> Bytes {
        BytesMut::with_capacity(Self::size()).freeze()
    }
}

struct GetSerialNumberReply {
    serial: [u8; 4],
}
impl EnttecPacket for GetSerialNumberReply {
    const LABEL: u8 = 10;
    fn size() -> usize {
        4
    }
}
impl TryFrom<Bytes> for GetSerialNumberReply {
    type Err = EnttecError;
    fn try_from(value: Bytes) -> Result<GetSerialNumberReply, EnttecError> {
        match Self::frame_check(value) {
            Ok(frame) => {
                let slice = Self::frame_data(&frame);
                if slice.len() == 4 {
                    let mut serial = [0u8; 4];
                    &serial.copy_from_slice(slice);
                    Ok(GetSerialNumberReply { serial: serial })
                } else {
                    Err(EnttecError::Length)
                }
            }
            Err(e) => Err(e),
        }


    }
}
impl Into<Bytes> for GetSerialNumberReply {
    fn into(self) -> Bytes {
        let mut bytes = BytesMut::with_capacity(Self::size());
        bytes.put_slice(&self.serial);
        bytes.freeze()
    }
}


extern crate serial;
use serial::SerialPort;
impl<T> EnttecWidget for T where T: SerialPort {}

#[test]
fn it_works() {
    let mut widget = serial::open("COM3").unwrap();

    widget
        .set_timeout(std::time::Duration::from_secs(1))
        .unwrap();

    widget
        .reconfigure(&|settings| {
                         try!(settings.set_baud_rate(serial::Baud9600));
                         settings.set_char_size(serial::Bits8);
                         settings.set_parity(serial::ParityNone);
                         settings.set_stop_bits(serial::Stop1);
                         settings.set_flow_control(serial::FlowNone);
                         Ok(())
                     })
        .unwrap();

    let res: GetParametersReply = widget.send_packet(GetParameters { param_size: 0 }).unwrap();
    panic!("Result: {:?}", res);
}
