use anyhow::{anyhow, Result};
use bus::{Bus, BusReader};
use std::fs::{self, File};
use std::net::UdpSocket;
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::cell_info::CellInfo;
use crate::logger::log_dci;
use crate::logic::{
    check_not_stopped, wait_until_running, MainState, MessageCellInfo, MessageDci, NgControlState,
    CHANNEL_SYNC_SIZE, DEFAULT_WORKER_SLEEP_MS, DEFAULT_WORKER_SLEEP_US,
};
use crate::ngscope;
use crate::ngscope::config::NgScopeConfig;
use crate::ngscope::types::{Message, NgScopeCellDci};
use crate::ngscope::{
    ngscope_validate_server_check, ngscope_validate_server_send_initial, start_ngscope,
    stop_ngscope,
};
use crate::parse::{Arguments, FlattenedNgScopeArgs};
use crate::util::{determine_process_id, is_debug, print_debug, print_info};

const WAIT_FOR_TRIGGER_NGSCOPE_RESPONE_MS: u64 = 500;

#[derive(Clone, Copy, Debug, PartialEq)]
enum LocalDciState {
    Stop,
    SendInitial,
    WaitForServerAuth(u8),
    SuccessfulAuth,
    ListenForDci,
}

pub struct NgControlArgs {
    pub rx_app_state: BusReader<MainState>,
    pub tx_ngcontrol_state: SyncSender<NgControlState>,
    pub app_args: Arguments,
    pub rx_cell_info: BusReader<MessageCellInfo>,
    pub tx_dci: Bus<MessageDci>,
}

struct RunArgs {
    rx_app_state: BusReader<MainState>,
    tx_ngcontrol_state: SyncSender<NgControlState>,
    app_args: Arguments,
    rx_cell_info: BusReader<MessageCellInfo>,
    tx_dci_thread_handle: Option<SyncSender<LocalDciState>>,
    dci_thread_handle: Option<JoinHandle<()>>,
    ng_process_handle: Option<Child>,
}

struct RunArgsMovables {
    tx_dci: Bus<MessageDci>,
}

pub fn deploy_ngscope_controller(args: NgControlArgs) -> Result<JoinHandle<()>> {
    let mut run_args: RunArgs = RunArgs {
        rx_app_state: args.rx_app_state,
        tx_ngcontrol_state: args.tx_ngcontrol_state,
        app_args: args.app_args,
        rx_cell_info: args.rx_cell_info,
        tx_dci_thread_handle: None,
        dci_thread_handle: None,
        ng_process_handle: None,
    };
    let run_args_mov: RunArgsMovables = RunArgsMovables {
        tx_dci: args.tx_dci,
    };
    let builder = thread::Builder::new().name("[builder]".to_string());
    let thread = builder.spawn(move || {
        let _ = run(&mut run_args, run_args_mov);
        finish(run_args);
    })?;
    Ok(thread)
}

fn run(run_args: &mut RunArgs, run_args_mov: RunArgsMovables) -> Result<()> {
    let rx_app_state = &mut run_args.rx_app_state;
    let tx_ngcontrol_state = &mut run_args.tx_ngcontrol_state;
    let app_args = &run_args.app_args;
    let rx_cell_info = &mut run_args.rx_cell_info;
    let tx_dci = run_args_mov.tx_dci;

    tx_ngcontrol_state.send(NgControlState::Running)?;
    wait_for_running(rx_app_state, tx_ngcontrol_state)?;
    print_info(&format!(
        "[ngcontrol]: \t\tPID {:?}",
        determine_process_id()
    ));

    let ng_args = FlattenedNgScopeArgs::from_unflattened(app_args.clone().ngscope.unwrap())?;
    let mut ng_process_option: Option<Child> = None;
    let ngscope_config = NgScopeConfig {
        ..Default::default()
    };

    let (tx_dci_thread, rx_main_thread) = sync_channel::<LocalDciState>(CHANNEL_SYNC_SIZE);
    let (tx_main_thread, rx_dci_thread) = sync_channel::<LocalDciState>(CHANNEL_SYNC_SIZE);
    run_args.dci_thread_handle = Some(deploy_dci_fetcher_thread(
        tx_main_thread,
        rx_main_thread,
        tx_dci,
        ng_args.ng_local_addr.to_string(),
        ng_args.ng_server_addr.to_string(),
        ng_args.ng_log_dci,
        ng_args.ng_log_dci_batch_size,
    )?);
    run_args.tx_dci_thread_handle = Some(tx_dci_thread.clone());

    let mut ngcontrol_state: NgControlState = NgControlState::CheckingCellInfo;

    loop {
        /* <precheck> */
        thread::sleep(Duration::from_millis(DEFAULT_WORKER_SLEEP_MS));
        if check_not_stopped(rx_app_state).is_err() {
            break;
        }
        /* </precheck> */

        match ngcontrol_state {
            NgControlState::CheckingCellInfo => {
                ngcontrol_state = handle_cell_update(rx_cell_info, &ngscope_config)?;
            }
            NgControlState::TriggerListenDci => {
                tx_dci_thread.send(LocalDciState::SendInitial)?;
                ngcontrol_state = NgControlState::WaitForTriggerResponse;
            }
            NgControlState::WaitForTriggerResponse => {
                ngcontrol_state = match rx_dci_thread.try_recv() {
                    Ok(LocalDciState::SuccessfulAuth) => NgControlState::SuccessfulTriggerResponse,
                    Ok(_) | Err(TryRecvError::Empty) => NgControlState::SleepMs(
                        WAIT_FOR_TRIGGER_NGSCOPE_RESPONE_MS,
                        Box::new(NgControlState::WaitForTriggerResponse),
                    ),
                    Err(TryRecvError::Disconnected) => {
                        print_info("[ngcontrol]: dci_fetcher thread disconnected unexpectedly while waiting for trigger response");
                        break;
                    }
                }
            }
            NgControlState::SuccessfulTriggerResponse => {
                tx_ngcontrol_state.send(NgControlState::SuccessfulTriggerResponse)?;
                ngcontrol_state = NgControlState::CheckingCellInfo
            }
            NgControlState::SleepMs(time_ms, next_state) => {
                thread::sleep(Duration::from_millis(time_ms));
                ngcontrol_state = *next_state;
            }
            NgControlState::StartNgScope(config) => {
                if let Some(ref mut process) = ng_process_option {
                    stop_ngscope(process)?;
                }
                match handle_start_ngscope(&config, &ng_args) {
                    Ok((state, proc)) => {
                        ngcontrol_state = state;
                        ng_process_option = proc;
                    }
                    Err(err) => {
                        print_info(&format!(
                            "ERROR [ngcontrol] could not start NG-Scope process: {:?}",
                            err
                        ));
                        print_info("ERROR [ngcontrol] retrying in 2 seconds..");
                        ngcontrol_state = NgControlState::SleepMs(
                            2000,
                            Box::new(NgControlState::StartNgScope(Box::new(*config.clone()))),
                        )
                    }
                }
            }
            NgControlState::StopNgScope => {
                if let Some(ref mut process) = ng_process_option {
                    stop_ngscope(process)?;
                }
                ngcontrol_state = NgControlState::CheckingCellInfo;
            }
            _ => todo!(),
        }
    }

    Ok(())
}

fn finish(mut run_args: RunArgs) {
    let _ = run_args
        .tx_ngcontrol_state
        .send(NgControlState::StoppingDciFetcherThread);

    if let Some(tx_dci_thread) = run_args.tx_dci_thread_handle {
        let _ = tx_dci_thread.send(LocalDciState::Stop);
    }

    if let Some(dci_thread) = run_args.dci_thread_handle {
        let _ = dci_thread.join();
    }
    if let Some(ref mut process) = run_args.ng_process_handle {
        let _ = run_args
            .tx_ngcontrol_state
            .send(NgControlState::StoppingNgScopeProcess);
        let _ = stop_ngscope(process);
    }
    let _ = send_final_state(&run_args.tx_ngcontrol_state);
}

fn handle_start_ngscope(
    ng_conf: &NgScopeConfig,
    ng_args: &FlattenedNgScopeArgs,
) -> Result<(NgControlState, Option<Child>)> {
    let (std_out, std_err) = match &ng_args.ng_log_file {
        Some(path) => {
            if Path::new(path).exists() {
                fs::remove_file(path).unwrap();
            }
            let file_out = File::create(path)?;
            let file_err = file_out.try_clone()?;
            (Stdio::from(file_out), Stdio::from(file_err))
        }
        None => (Stdio::null(), Stdio::null()),
    };
    let new_ng_process = match ng_args.ng_start_process {
        true => Some(start_ngscope(&ng_args.ng_path, ng_conf, std_out, std_err)?),
        false => None,
    };
    Ok((
        NgControlState::SleepMs(5000, Box::new(NgControlState::TriggerListenDci)),
        new_ng_process,
    ))
}

fn handle_cell_update(
    rx_cell_info: &mut BusReader<MessageCellInfo>,
    ng_conf: &NgScopeConfig,
) -> Result<NgControlState> {
    match check_cell_update(rx_cell_info)? {
        Some(cell_info) => {
            print_info(&format!("[ngcontrol] cell_info: {:#?}", cell_info));
            if cell_info.cells.is_empty() {
                return Ok(NgControlState::StopNgScope);
            }
            // TODO: Handle multi cell
            let mut new_conf = ng_conf.clone();
            new_conf.rf_config0.as_mut().unwrap().rf_freq =
                cell_info.cells.first().unwrap().frequency as i64;
            Ok(NgControlState::StartNgScope(Box::new(new_conf)))
        }
        _ => Ok(NgControlState::CheckingCellInfo),
    }
}

fn deploy_dci_fetcher_thread(
    tx_main_thread: SyncSender<LocalDciState>,
    rx_main_thread: Receiver<LocalDciState>,
    tx_dci: Bus<MessageDci>,
    local_socket_addr: String,
    ng_server_addr: String,
    is_log_dci: bool,
    log_dci_batch_size: u64,
) -> Result<JoinHandle<()>> {
    let thread = thread::spawn(move || {
        let _ = run_dci_fetcher(
            tx_main_thread,
            rx_main_thread,
            tx_dci,
            local_socket_addr,
            ng_server_addr,
            is_log_dci,
            log_dci_batch_size,
        );
    });
    Ok(thread)
}

fn run_dci_fetcher(
    tx_main_thread: SyncSender<LocalDciState>,
    rx_main_thread: Receiver<LocalDciState>,
    mut tx_dci: Bus<MessageDci>,
    local_socket_addr: String,
    ng_server_addr: String,
    is_log_dci: bool,
    log_dci_batch_size: u64,
) -> Result<()> {
    let socket = init_dci_server(&local_socket_addr)?;
    let mut dci_state: LocalDciState = LocalDciState::ListenForDci;
    print_info(&format!(
        "[ngcontrol.dci]: \tPID {:?}",
        determine_process_id()
    ));

    let mut log_dci_buffer: Vec<NgScopeCellDci> =
        Vec::with_capacity(2 * log_dci_batch_size as usize);
    let sleep_duration = Duration::from_micros(DEFAULT_WORKER_SLEEP_US);
    let mut last_dci_timestamp_us: u64 = 0;

    loop {
        thread::sleep(sleep_duration);
        if let Some(new_state) = check_rx_state(&rx_main_thread)? {
            dci_state = new_state;
        }

        match dci_state {
            LocalDciState::Stop => {
                break;
            }
            LocalDciState::SendInitial => {
                dci_state = match ngscope_validate_server_send_initial(&socket, &ng_server_addr) {
                    Ok(_) => LocalDciState::WaitForServerAuth(0),
                    Err(_) => LocalDciState::SendInitial,
                };
            }
            LocalDciState::SuccessfulAuth => {
                tx_main_thread.send(LocalDciState::SuccessfulAuth)?;
                dci_state = LocalDciState::ListenForDci;
            }
            LocalDciState::WaitForServerAuth(successful_auths) => {
                dci_state = match ngscope_validate_server_check(&socket)? {
                    Some(_) => {
                        if successful_auths >= 1 {
                            LocalDciState::SuccessfulAuth
                        } else {
                            LocalDciState::WaitForServerAuth(successful_auths + 1)
                        }
                    }
                    None => LocalDciState::WaitForServerAuth(successful_auths),
                };
            }
            LocalDciState::ListenForDci => {
                check_ngscope_message(
                    &socket,
                    &mut tx_dci,
                    &mut last_dci_timestamp_us,
                    &is_log_dci,
                    &mut log_dci_buffer,
                );
                check_log_dci(&is_log_dci, &mut log_dci_buffer, &log_dci_batch_size);
            }
        }
    }
    Ok(())
}

fn check_ngscope_message(
    socket: &UdpSocket,
    tx_dci: &mut Bus<MessageDci>,
    last_dci_timestamp_us: &mut u64,
    is_log_dci: &bool,
    log_dci_buffer: &mut Vec<NgScopeCellDci>,
) {
    match ngscope::ngscope_recv_single_message(socket) {
        Ok(msg) => {
            match msg {
                Message::CellDci(cell_dci) => {
                    if is_debug() {
                        if *last_dci_timestamp_us != 0
                            && cell_dci.time_stamp > *last_dci_timestamp_us
                        {
                            let timestamp_delta: i64 =
                                cell_dci.time_stamp as i64 - *last_dci_timestamp_us as i64;
                            if timestamp_delta > 1000000 {
                                let now_delta = chrono::Local::now().timestamp_micros() as u64
                                    - cell_dci.time_stamp;
                                print_debug(&format!(
                                    "DEBUG [ngcontrol.fetcher] 1s DCI gap:\n\
                                                      \tdiff to previous DCI: {:>10} us\n\
                                                      \tdiff to now:          {:>10} us",
                                    timestamp_delta, now_delta
                                ));
                            }
                        }
                        *last_dci_timestamp_us = cell_dci.time_stamp;
                    }
                    /* check bus size */
                    let message_dci = MessageDci {
                        ngscope_dci: *cell_dci,
                    };
                    if *is_log_dci {
                        log_dci_buffer.push(*cell_dci.clone());
                    }
                    match tx_dci.try_broadcast(message_dci) {
                        Ok(_) => {}
                        Err(msg) => {
                            print_info("ERROR [ngcontrol] DCI bus is full!!");
                            tx_dci.broadcast(msg)
                        }
                    }
                }
                Message::Dci(ue_dci) => {
                    // TODO: Evaluate how to handle this
                    print_info(&format!("[ngcontrol] {:?}", ue_dci));
                }
                Message::Config(cell_config) => {
                    // TODO: Evaluate how to handle this
                    print_info(&format!("[ngcontrol] {:?}", cell_config));
                }
                // TODO: Evaluate how to handle Start andExit
                Message::Start => {}
                Message::Exit => {}
            }
        }
        _ => {
            // TODO: print error properly? it also goes here when there just hasn't been a message
        }
    }
}

fn check_log_dci(
    is_log_dci: &bool,
    log_dci_buffer: &mut Vec<NgScopeCellDci>,
    log_dci_batch_size: &u64,
) {
    if *is_log_dci && log_dci_buffer.len() >= *log_dci_batch_size as usize {
        let _ = log_dci(log_dci_buffer.clone());
        log_dci_buffer.clear()
    }
}

/*  --------------  */
/*      Helpers     */
/*  --------------  */

fn check_rx_state(rx_local_dci_state: &Receiver<LocalDciState>) -> Result<Option<LocalDciState>> {
    match rx_local_dci_state.try_recv() {
        Ok(msg) => Ok(Some(msg)),
        Err(TryRecvError::Empty) => Ok(None),
        Err(TryRecvError::Disconnected) => Err(anyhow!(
            "[ngcontrol.local_dci_fetcher] rx_local_dci_state disconnected"
        )),
    }
}

fn init_dci_server(local_addr: &str) -> Result<UdpSocket> {
    let socket = UdpSocket::bind(local_addr)?;
    socket.set_nonblocking(true)?;

    Ok(socket)
}

fn send_final_state(tx_ngcontrol_state: &SyncSender<NgControlState>) -> Result<()> {
    Ok(tx_ngcontrol_state.send(NgControlState::Stopped)?)
}

fn wait_for_running(
    rx_app_state: &mut BusReader<MainState>,
    tx_ngcontrol_state: &SyncSender<NgControlState>,
) -> Result<()> {
    match wait_until_running(rx_app_state) {
        Ok(_) => Ok(()),
        _ => {
            send_final_state(tx_ngcontrol_state)?;
            Err(anyhow!("[source] Main did not send 'Running' message"))
        }
    }
}

fn check_cell_update(rx_cell_info: &mut BusReader<MessageCellInfo>) -> Result<Option<CellInfo>> {
    match rx_cell_info.try_recv() {
        Ok(msg) => Ok(Some(msg.cell_info)),
        Err(TryRecvError::Disconnected) => {
            // TODO: print error properly
            Err(anyhow!("[ngcontrol] err: rx_cell_info disconnected!"))
        }
        Err(TryRecvError::Empty) => Ok(None),
    }
}
