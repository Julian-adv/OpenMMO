use std::{env, io, time::Duration};
use tracing::warn;

pub struct Notifier {
    #[cfg(unix)]
    socket: Option<std::os::unix::net::UnixDatagram>,
    interval: Duration,
}

impl Notifier {
    pub fn from_env() -> io::Result<Self> {
        let interval = match env::var("WATCHDOG_USEC") {
            Ok(value) => {
                let micros: u64 = value
                    .parse()
                    .map_err(|_| io::Error::other("Invalid WATCHDOG_USEC"))?;
                if micros == 0 {
                    Duration::from_secs(5)
                } else {
                    Duration::from_micros((micros / 2).clamp(1, 5_000_000))
                }
            }
            Err(_) => Duration::from_secs(5),
        };
        if let Ok(pid) = env::var("WATCHDOG_PID") {
            if pid.parse::<u32>().ok() != Some(std::process::id()) {
                return Err(io::Error::other("WATCHDOG_PID does not match this process"));
            }
        }
        #[cfg(unix)]
        let socket = env::var_os("NOTIFY_SOCKET")
            .map(|path| connect(&path))
            .transpose()?;
        Ok(Self {
            #[cfg(unix)]
            socket,
            interval,
        })
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Returns whether the message reached systemd; failures are logged.
    pub fn send(&self, message: &str) -> bool {
        #[cfg(unix)]
        if let Some(socket) = &self.socket {
            if let Err(error) = socket.send(message.as_bytes()) {
                warn!("Failed to notify systemd: {error}");
                return false;
            }
        }
        #[cfg(not(unix))]
        let _ = message;
        true
    }
}

#[cfg(unix)]
fn connect(path: &std::ffi::OsStr) -> io::Result<std::os::unix::net::UnixDatagram> {
    use std::os::unix::{ffi::OsStrExt, net::UnixDatagram};
    let socket = UnixDatagram::unbound()?;
    socket.set_nonblocking(true)?;
    if let Some(name) = path.as_bytes().strip_prefix(b"@") {
        #[cfg(target_os = "linux")]
        {
            use std::os::{linux::net::SocketAddrExt, unix::net::SocketAddr};
            socket.connect_addr(&SocketAddr::from_abstract_name(name)?)?;
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = name;
            return Err(io::Error::other("Abstract notify sockets require Linux"));
        }
    } else {
        socket.connect(path)?;
    }
    Ok(socket)
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;
    use std::os::{
        linux::net::SocketAddrExt,
        unix::net::{SocketAddr, UnixDatagram},
    };

    fn pair() -> (Notifier, UnixDatagram) {
        let name = format!("openmmo-notify-test-{}", uuid::Uuid::new_v4());
        let receiver =
            UnixDatagram::bind_addr(&SocketAddr::from_abstract_name(name.as_bytes()).unwrap())
                .unwrap();
        receiver.set_nonblocking(true).unwrap();
        let socket = connect(std::ffi::OsStr::new(&format!("@{name}"))).unwrap();
        (
            Notifier {
                socket: Some(socket),
                interval: Duration::from_secs(5),
            },
            receiver,
        )
    }

    #[tokio::test(start_paused = true)]
    async fn monitor_withholds_keepalives_on_stall_and_stops_on_shutdown() {
        use crate::health::{
            tests::{complete_all, register_three},
            Health,
        };
        let health = Health::new();
        let checkpoints = register_three(&health);
        let (notifier, receiver) = pair();
        let (shutdown, rx) = tokio::sync::watch::channel(());
        let task = tokio::spawn(health.clone().monitor(std::sync::Arc::new(notifier), rx));
        tokio::task::yield_now().await;
        let mut buffer = [0u8; 256];
        assert_eq!(
            receiver.recv(&mut buffer).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        complete_all(&checkpoints);
        tokio::time::advance(Duration::from_secs(5)).await;
        tokio::task::yield_now().await;
        let count = receiver.recv(&mut buffer).unwrap();
        assert!(std::str::from_utf8(&buffer[..count])
            .unwrap()
            .contains("READY=1\nWATCHDOG=1"));
        tokio::time::advance(Duration::from_secs(25)).await;
        checkpoints[2].completed();
        tokio::task::yield_now().await;
        let count = receiver.recv(&mut buffer).unwrap();
        let message = std::str::from_utf8(&buffer[..count]).unwrap();
        assert!(message.starts_with("STATUS=Server progress stalled:"));
        assert!(!message.contains("WATCHDOG=1"));
        assert_eq!(
            receiver.recv(&mut buffer).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        complete_all(&checkpoints);
        tokio::time::advance(Duration::from_secs(5)).await;
        tokio::task::yield_now().await;
        let count = receiver.recv(&mut buffer).unwrap();
        let message = std::str::from_utf8(&buffer[..count]).unwrap();
        assert!(message.starts_with("WATCHDOG=1"));
        assert!(!message.contains("READY=1"));
        health.stop();
        shutdown.send(()).unwrap();
        task.await.unwrap();
        assert_eq!(
            receiver.recv(&mut buffer).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn sends_to_filesystem_socket() {
        let path = crate::test_util::unique_temp_dir("notify");
        let receiver = UnixDatagram::bind(&path).unwrap();
        receiver.set_nonblocking(true).unwrap();
        let notifier = Notifier {
            socket: Some(connect(path.as_os_str()).unwrap()),
            interval: Duration::from_secs(5),
        };
        assert!(notifier.send("STOPPING=1"));
        let mut buffer = [0; 32];
        let size = receiver.recv(&mut buffer).unwrap();
        assert_eq!(&buffer[..size], b"STOPPING=1");
        std::fs::remove_file(path).unwrap();
    }
}
