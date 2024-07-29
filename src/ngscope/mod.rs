use anyhow::{anyhow, Result};

use std::net::UdpSocket;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

#[allow(dead_code)]
pub mod config;
#[allow(dead_code)]
pub mod types;

use config::NgScopeConfig;
use types::{Message, MessageType};

use crate::util::print_info;

const TMP_NGSCOPE_CONFIG_PATH: &str = "./.tmp_ngscope_conf.cfg";

pub fn start_ngscope<T: Into<Stdio>>(
    exec_path: &str,
    config: &NgScopeConfig,
    proc_stdout: T,
    proc_stderr: T,
) -> Result<Child> {
    serde_libconfig::to_file(config, TMP_NGSCOPE_CONFIG_PATH)?;
    let child = Command::new(exec_path)
        .stdout(proc_stdout)
        .stderr(proc_stderr)
        .arg("-c")
        .arg(TMP_NGSCOPE_CONFIG_PATH)
        .spawn()?;
    Ok(child)
}

pub fn stop_ngscope(child: &mut Child) -> Result<()> {
    child.kill()?;
    Ok(())
}

#[allow(dead_code)]
pub fn restart_ngscope<T: Into<Stdio>>(
    child: &mut Child,
    exec_path: &str,
    config: &NgScopeConfig,
    proc_stdout: T,
    proc_stderr: T,
) -> Result<Child> {
    stop_ngscope(child)?;
    thread::sleep(Duration::from_secs(3));
    let new_child = start_ngscope(exec_path, config, proc_stdout, proc_stderr)?;
    Ok(new_child)
}

pub fn ngscope_recv_single_message_type(socket: &UdpSocket) -> Result<(MessageType, Vec<u8>)> {
    let mut buf = [0u8; types::NGSCOPE_REMOTE_BUFFER_SIZE];
    loop {
        if let Ok((nof_recv, _)) = socket.recv_from(&mut buf) {
            return types::ngscope_extract_packet(&buf[..nof_recv]);
        }
    }
}

pub fn ngscope_recv_single_message(socket: &UdpSocket) -> Result<Message> {
    let mut buf = [0u8; types::NGSCOPE_REMOTE_BUFFER_SIZE];
    let (nof_recv, _) = socket.recv_from(&mut buf)?;
    Message::from_bytes(&buf[..nof_recv])
}

#[allow(dead_code)]
pub fn ngscope_validate_server(socket: &UdpSocket, server_addr: &str) -> Result<()> {
    let init_sequence = MessageType::Start.to_bytes();
    socket
        .send_to(&init_sequence, server_addr)
        .expect("error sending init sequence");

    let mut nof_messages_to_validate = types::NOF_VALIDATE_SUCCESS;
    for _ in 0..types::NOF_VALIDATE_RETRIES {
        if nof_messages_to_validate < 1 {
            return Ok(());
        }
        let msg_type = ngscope_recv_single_message_type(socket);
        match msg_type {
            Ok((msg_type, _)) => match msg_type {
                MessageType::Start
                | MessageType::Dci
                | MessageType::CellDci
                | MessageType::Config => nof_messages_to_validate -= 1,
                MessageType::Exit => break,
            },
            Err(err) => print_info(&format!("failed evaluating message, retrying... `{}`", err)),
        }
    }

    Err(anyhow!(
        "Could not validate message within {} tries",
        types::NOF_VALIDATE_RETRIES
    ))
}

pub fn ngscope_validate_server_send_initial(socket: &UdpSocket, server_addr: &str) -> Result<()> {
    let init_sequence = MessageType::Start.to_bytes();
    socket.send_to(&init_sequence, server_addr)?;
    Ok(())
}

pub fn ngscope_validate_server_check(socket: &UdpSocket) -> Result<Option<Message>> {
    let msg = ngscope_recv_single_message(socket);
    match msg {
        Ok(msg) => match msg {
            Message::Exit => Err(anyhow!(
                "Received Exit from ngscope server during validation"
            )),
            Message::Start |
            Message::Dci(_) |
            Message::CellDci(_) |
            Message::Config(_) => { Ok(Some(msg)) }
        },
        Err(_) => Ok(None),
    }
}
