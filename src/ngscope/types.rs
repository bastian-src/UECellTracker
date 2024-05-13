use anyhow::{anyhow, Context, Result};

pub const NOF_VALIDATE_RETRIES: usize = 50;
pub const NOF_VALIDATE_SUCCESS: usize = 2;

// taken from: ngscope/src/dciLib/dci_sink_client.c
pub const NGSCOPE_REMOTE_BUFFER_SIZE: usize = 1400; // ngscope sending buffer size
                                                    // taken from: ngscope/hdr/dciLib/dci_sink_def.h
pub const NGSCOPE_MAX_NOF_CELL: usize = 4; // ngscope max nof cells per station
pub const NGSCOPE_MAX_NOF_RNTI: usize = 20;

pub const NGSCOPE_MESSAGE_TYPE_SIZE: usize = 4;
pub const NGSCOPE_MESSAGE_VERSION_POSITION: usize = 4;
pub const NGSCOPE_MESSAGE_CONTENT_POSITION: usize = 5;
pub const NGSCOPE_STRUCT_SIZE_DCI: usize = 40;
pub const NGSCOPE_STRUCT_SIZE_CELL_DCI: usize = 448;
pub const NGSCOPE_STRUCT_SIZE_CONFIG: usize = 12; // TODO: Determine this actually

// IMPORTANT:
// - when receiving messages, check the timestamp - due to UDP, messages might arrive out of order
// but the timestamp saves us.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MessageType {
    Start,
    Dci,
    CellDci,
    Config,
    Exit,
}

#[derive(Clone, Debug)]
pub enum Message {
    Start,
    Dci(NgScopeUeDci),
    CellDci(Box<NgScopeCellDci>),
    Config(NgScopeCellConfig),
    Exit,
}

impl Message {
    pub fn from_bytes(bytes: &[u8]) -> Result<Message> {
        if bytes.len() < NGSCOPE_MESSAGE_TYPE_SIZE {
            return Err(anyhow!(
                "bytes must be at least {}",
                NGSCOPE_MESSAGE_TYPE_SIZE
            ));
        }
        let msg_type_bytes: [u8; NGSCOPE_MESSAGE_TYPE_SIZE] =
            bytes[..NGSCOPE_MESSAGE_TYPE_SIZE].try_into().unwrap();
        let _version_byte: u8 = bytes[NGSCOPE_MESSAGE_VERSION_POSITION];
        let content_bytes: &[u8] = &bytes[NGSCOPE_MESSAGE_CONTENT_POSITION..];
        let msg: Message = match MessageType::from_bytes(&msg_type_bytes).unwrap() {
            MessageType::Start => Message::Start,
            MessageType::Dci => Message::Dci(NgScopeUeDci::from_bytes(content_bytes.try_into()?)?),
            MessageType::CellDci => {
                Message::CellDci(Box::new(NgScopeCellDci::from_bytes(content_bytes.try_into()?)?))
            }
            MessageType::Config => {
                Message::Config(NgScopeCellConfig::from_bytes(content_bytes.try_into()?)?)
            }
            MessageType::Exit => Message::Exit,
        };
        Ok(msg)
    }
}

impl MessageType {
    pub fn from_bytes(bytes: &[u8; NGSCOPE_MESSAGE_TYPE_SIZE]) -> Option<MessageType> {
        match *bytes {
            [0xCC, 0xCC, 0xCC, 0xCC] => Some(MessageType::Start),
            [0xAA, 0xAA, 0xAA, 0xAA] => Some(MessageType::Dci),
            [0xAB, 0xAB, 0xAB, 0xAB] => Some(MessageType::CellDci),
            [0xBB, 0xBB, 0xBB, 0xBB] => Some(MessageType::Config),
            [0xFF, 0xFF, 0xFF, 0xFF] => Some(MessageType::Exit),
            _ => None,
        }
    }
    pub fn to_bytes(self) -> [u8; NGSCOPE_MESSAGE_TYPE_SIZE] {
        match self {
            MessageType::Start => [0xCC; NGSCOPE_MESSAGE_TYPE_SIZE],
            MessageType::Dci => [0xAA; NGSCOPE_MESSAGE_TYPE_SIZE],
            MessageType::CellDci => [0xAB; NGSCOPE_MESSAGE_TYPE_SIZE],
            MessageType::Config => [0xBB; NGSCOPE_MESSAGE_TYPE_SIZE],
            MessageType::Exit => [0xFF; NGSCOPE_MESSAGE_TYPE_SIZE],
        }
    }
}

// taken from: ngscope/hdr/dciLib/dci_sink_def.h
#[repr(C)]
#[derive(Clone, Debug)]
pub struct NgScopeUeDci {
    pub cell_idx: u8,
    pub time_stamp: u64,
    pub tti: u16,
    pub rnti: u16,
    // downlink information
    pub dl_tbs: u32,
    pub dl_re_tx: u8,
    pub dl_rv_flag: bool,
    // uplink information
    pub ul_tbs: u32,
    pub ul_re_tx: u8,
    pub ul_rv_flag: bool,
}

impl NgScopeUeDci {
    pub fn from_bytes(bytes: [u8; NGSCOPE_STRUCT_SIZE_DCI]) -> Result<NgScopeUeDci> {
        let ue_dci: &NgScopeUeDci = unsafe { &*bytes.as_ptr().cast() };
        Ok(ue_dci.clone())
    }
}

#[repr(C)]
#[derive(Clone, Debug)]
#[allow(non_snake_case)]
pub struct NgScopeRntiDci {
    pub rnti: u16,
	pub dl_tbs: u32,
	pub dl_prb: u8,
	pub dl_reTx: u8,

	pub ul_tbs: u32,
	pub ul_prb: u8,
	pub ul_reTx: u8,
}

#[repr(C)]
#[derive(Clone, Debug)]
#[allow(non_snake_case)]
pub struct NgScopeCellDci {
	pub cell_id: u8,
	pub time_stamp: u64,
	pub tti: u16,
	pub total_dl_tbs: u64,
	pub total_ul_tbs: u64,
	pub total_dl_prb: u8,
	pub total_ul_prb: u8,
	pub total_dl_reTx: u8,
	pub total_ul_reTx: u8,
	pub nof_rnti: u8,
    pub rnti_list: [NgScopeRntiDci; NGSCOPE_MAX_NOF_RNTI],
}

impl NgScopeCellDci {
    pub fn from_bytes(bytes: [u8; NGSCOPE_STRUCT_SIZE_CELL_DCI]) -> Result<NgScopeCellDci> {
        let cell_dci: &NgScopeCellDci = unsafe { &*bytes.as_ptr().cast() };
        Ok(cell_dci.clone())
    }
}

// taken from: ngscope/hdr/dciLib/dci_sink_def.h
#[repr(C)]
#[derive(Clone, Debug)]
pub struct NgScopeCellConfig {
    pub nof_cell: u8,
    pub cell_prb: [u16; NGSCOPE_MAX_NOF_CELL],
    pub rnti: u16,
}

impl NgScopeCellConfig {
    pub fn from_bytes(bytes: [u8; NGSCOPE_STRUCT_SIZE_CONFIG]) -> Result<NgScopeCellConfig> {
        let ue_dci: &NgScopeCellConfig = unsafe { &*bytes.as_ptr().cast() };
        Ok(ue_dci.clone())
    }
}

pub fn ngscope_extract_packet(packet: &[u8]) -> Result<(MessageType, Vec<u8>)> {
    if packet.len() < NGSCOPE_MESSAGE_TYPE_SIZE {
        return Err(anyhow!(
            "Packet size has to be at least {}.",
            NGSCOPE_MESSAGE_TYPE_SIZE
        ));
    }

    let (preamble, content) = packet.split_at(NGSCOPE_MESSAGE_TYPE_SIZE);
    let msg_type = MessageType::from_bytes(preamble.try_into()?)
        .context("Could not extrat message type form bytes.")?;
    Ok((msg_type, content.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMMY_CONFIG: [u8; 16] = [187, 187, 187, 187, 1, 0, 50, 0, 0, 0, 0, 0, 0, 0, 71, 74];
    const DUMMY_DCI: [u8; 45] = [
        170, 170, 170, 170, 0, 0, 0, 0, 0, 0, 0, 0, 0, 24, 108, 27, 83, 116, 18, 6, 0, 211, 31, 71,
        74, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0,
    ];
    const DUMMY_START: [u8; 4] = [204, 204, 204, 204];
    const DUMMY_EXIT: [u8; 4] = [255, 255, 255, 255];

    #[test]
    fn test_extract_config() {
        let result = ngscope_extract_packet(&DUMMY_CONFIG);
        assert!(result.is_ok());
        if let Ok((msg_type, content)) = result {
            assert_eq!(msg_type, MessageType::Config);
            assert_eq!(content, [1, 0, 50, 0, 0, 0, 0, 0, 0, 0, 71, 74].to_vec());
        }
    }

    #[test]
    fn test_extract_dci() {
        let result = ngscope_extract_packet(&DUMMY_DCI);
        assert!(result.is_ok());
        if let Ok((msg_type, content)) = result {
            assert_eq!(msg_type, MessageType::Dci);
            assert_eq!(
                content,
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 24, 108, 27, 83, 116, 18, 6, 0, 211, 31, 71, 74, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0
                ]
                .to_vec()
            );
        }
    }

    #[test]
    fn test_extract_start() {
        let result = ngscope_extract_packet(&DUMMY_START);
        let empty_vec: Vec<u8> = vec![];
        assert!(result.is_ok());
        if let Ok((msg_type, content)) = result {
            assert_eq!(msg_type, MessageType::Start);
            assert_eq!(content, empty_vec);
        }
    }

    #[test]
    fn test_extract_exit() {
        let result = ngscope_extract_packet(&DUMMY_EXIT);
        let empty_vec: Vec<u8> = vec![];
        assert!(result.is_ok());
        if let Ok((msg_type, content)) = result {
            assert_eq!(msg_type, MessageType::Exit);
            assert_eq!(content, empty_vec);
        }
    }

    #[test]
    fn test_extract_err_too_few_bytes() {
        let result = ngscope_extract_packet(&[123, 123]);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_err_b() {
        let result = ngscope_extract_packet(&[255, 255, 123, 234, 123]);
        assert!(result.is_err());
    }
}
