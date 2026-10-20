//! Starting Shot while it already runs opens the running copy's Settings
//! window. The running copy listens on a loopback-only port (never reachable
//! from the network) whose number it writes next to the settings file.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use winit::event_loop::EventLoopProxy;

use super::app::UserEvent;

fn port_file() -> std::path::PathBuf {
    shot::config::app_dir().join("shot.port")
}

pub fn listen(proxy: EventLoopProxy<UserEvent>) {
    let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) else { return };
    let Ok(addr) = listener.local_addr() else { return };
    if std::fs::write(port_file(), addr.port().to_string()).is_err() {
        return;
    }
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let mut line = String::new();
            // Only one short line is ever expected; cap what a stray client can send.
            if BufReader::new(stream.take(64)).read_line(&mut line).is_ok() && line.trim() == "shot:settings" {
                let _ = proxy.send_event(UserEvent::OpenSettings);
            }
        }
    });
}

/// Ask the running copy to show its Settings window. False if none answered.
pub fn ask_running_to_open_settings() -> bool {
    let Some(port) = std::fs::read_to_string(port_file()).ok().and_then(|s| s.trim().parse::<u16>().ok()) else { return false };
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    match TcpStream::connect_timeout(&addr, Duration::from_millis(400)) {
        Ok(mut s) => s.write_all(b"shot:settings\n").is_ok(),
        Err(_) => false,
    }
}
