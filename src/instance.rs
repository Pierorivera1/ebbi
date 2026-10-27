use crate::app::Screen;
use iced::futures::{channel::mpsc, SinkExt, StreamExt};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

pub struct Instance {
    _lock: File,
    socket: PathBuf,
    stopped: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    receiver: Arc<Mutex<Option<mpsc::Receiver<Screen>>>>,
}

pub fn database_path(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() || path == Path::new(":memory:") {
        return Err("A persistent database path is required".into());
    }
    if path.exists() {
        return fs::canonicalize(path).map_err(|e| e.to_string());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|e| e.to_string())?;
    Ok(fs::canonicalize(parent)
        .map_err(|e| e.to_string())?
        .join(path.file_name().ok_or("Missing database filename")?))
}

impl Instance {
    pub fn start(path: &Path, screen: Screen) -> Result<Option<Self>, String> {
        let root = dirs::runtime_dir()
            .or_else(dirs::cache_dir)
            .ok_or("Cannot locate a private IPC directory")?
            .join("ebbi-ipc");
        Self::start_in(path, screen, &root)
    }

    fn start_in(path: &Path, screen: Screen, root: &Path) -> Result<Option<Self>, String> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
            .map_err(|e| e.to_string())?;
        let metadata = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
        let uid = fs::metadata("/proc/self").map_err(|e| e.to_string())?.uid();
        if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
            return Err("Ebbi IPC directory must be owned by you and private (mode 0700)".into());
        }
        let key = format!(
            "{:x}",
            Sha256::digest(database_path(path)?.as_os_str().as_bytes())
        );
        let socket = root.join(format!("{}.sock", &key[..32]));
        let lock_path = socket.with_extension("lock");
        if fs::symlink_metadata(&lock_path).is_ok_and(|m| !m.is_file() || m.uid() != uid) {
            return Err("Unsafe Ebbi instance lock".into());
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(lock_path)
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) => {
                    match forward(&socket, screen, deadline) {
                        Ok(()) => return Ok(None),
                        Err(error) if Instant::now() >= deadline => {
                            return Err(format!(
                        "Ebbi is already running but could not receive the view request: {error}"
                    ))
                        }
                        Err(_) => thread::sleep(Duration::from_millis(20)),
                    }
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        if let Ok(metadata) = fs::symlink_metadata(&socket) {
            if !metadata.file_type().is_socket() || metadata.uid() != uid {
                return Err("Unsafe Ebbi socket path; refusing to replace it".into());
            }
            fs::remove_file(&socket).map_err(|e| e.to_string())?;
        }
        let listener = UnixListener::bind(&socket).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let (mut sender, receiver) = mpsc::channel(16);
        let thread = thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let timeout = Some(Duration::from_millis(500));
                        if stream.set_read_timeout(timeout).is_err()
                            || stream.set_write_timeout(timeout).is_err()
                        {
                            continue;
                        }
                        let mut request = [0];
                        let accepted = stream.read_exact(&mut request).is_ok()
                            && decode(request[0])
                                .is_some_and(|screen| sender.try_send(screen).is_ok());
                        let _ = stream.write_all(&[u8::from(accepted)]);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Some(Self {
            _lock: lock,
            socket,
            stopped,
            thread: Some(thread),
            receiver: Arc::new(Mutex::new(Some(receiver))),
        }))
    }

    pub fn subscription(&self) -> iced::Subscription<Screen> {
        let receiver = self.receiver.clone();
        iced::Subscription::run_with_id(
            "instance-routing",
            iced::stream::channel(1, move |mut output| async move {
                let receiver = receiver.lock().expect("IPC receiver lock poisoned").take();
                if let Some(mut receiver) = receiver {
                    while let Some(screen) = receiver.next().await {
                        if output.send(screen).await.is_err() {
                            break;
                        }
                    }
                }
            }),
        )
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.socket);
    }
}

fn forward(socket: &Path, screen: Screen, deadline: Instant) -> Result<(), String> {
    use rustix::net::{
        connect, socket_with, AddressFamily, SocketAddrUnix, SocketFlags, SocketType,
    };
    let fd = socket_with(
        AddressFamily::UNIX,
        SocketType::STREAM,
        SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
        None,
    )
    .map_err(|e| e.to_string())?;
    connect(
        &fd,
        &SocketAddrUnix::new(socket).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut stream = UnixStream::from(fd);
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    let timeout = Some(
        deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(500))
            .max(Duration::from_millis(1)),
    );
    stream
        .set_read_timeout(timeout)
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(timeout)
        .map_err(|e| e.to_string())?;
    stream
        .write_all(&[encode(screen)])
        .map_err(|e| e.to_string())?;
    let mut ack = [0];
    stream.read_exact(&mut ack).map_err(|e| e.to_string())?;
    if ack == [1] {
        Ok(())
    } else {
        Err("Existing Ebbi could not queue the view request; try again".into())
    }
}

fn encode(screen: Screen) -> u8 {
    match screen {
        Screen::Dashboard => 1,
        Screen::Study => 2,
        Screen::Analytics => 3,
        Screen::Settings => 4,
    }
}

fn decode(value: u8) -> Option<Screen> {
    match value {
        1 => Some(Screen::Dashboard),
        2 => Some(Screen::Study),
        3 => Some(Screen::Analytics),
        4 => Some(Screen::Settings),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_views_per_database_releases_socket_and_recovers_stale_socket() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ipc");
        let db = temp.path().join("test.db");
        let instance = Instance::start_in(&db, Screen::Dashboard, &root)
            .unwrap()
            .unwrap();
        let mut receiver = instance.receiver.lock().unwrap().take().unwrap();
        for screen in [
            Screen::Study,
            Screen::Analytics,
            Screen::Settings,
            Screen::Dashboard,
        ] {
            assert!(Instance::start_in(&db, screen, &root).unwrap().is_none());
            assert_eq!(
                iced::futures::executor::block_on(receiver.next()),
                Some(screen)
            );
        }
        fs::write(&db, []).unwrap();
        let alias = temp.path().join("alias.db");
        std::os::unix::fs::symlink(&db, &alias).unwrap();
        assert!(Instance::start_in(&alias, Screen::Study, &root)
            .unwrap()
            .is_none());
        assert_eq!(
            iced::futures::executor::block_on(receiver.next()),
            Some(Screen::Study)
        );
        let other = Instance::start_in(&temp.path().join("other.db"), Screen::Study, &root)
            .unwrap()
            .unwrap();
        let socket = instance.socket.clone();
        drop(instance);
        assert!(!socket.exists());
        drop(UnixListener::bind(&socket).unwrap());
        assert!(Instance::start_in(&db, Screen::Study, &root)
            .unwrap()
            .is_some());
        drop(other);
    }

    #[test]
    fn launch_during_shutdown_retries_until_instance_lock_is_released() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ipc");
        let db = temp.path().join("test.db");
        let mut instance = Instance::start_in(&db, Screen::Dashboard, &root)
            .unwrap()
            .unwrap();
        instance.stopped.store(true, Ordering::Relaxed);
        instance.thread.take().unwrap().join().unwrap();
        let closing = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            drop(instance);
        });
        assert!(Instance::start_in(&db, Screen::Study, &root)
            .unwrap()
            .is_some());
        closing.join().unwrap();
    }

    #[test]
    fn saturated_socket_backlog_does_not_block_connection() {
        use rustix::net::{
            connect, socket_with, AddressFamily, SocketAddrUnix, SocketFlags, SocketType,
        };
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("full.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        rustix::net::listen(&listener, 1).unwrap();
        let address = SocketAddrUnix::new(&socket).unwrap();
        let mut connections = Vec::new();
        let mut full = false;
        for _ in 0..1024 {
            let fd = socket_with(
                AddressFamily::UNIX,
                SocketType::STREAM,
                SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
                None,
            )
            .unwrap();
            match connect(&fd, &address) {
                Ok(()) => connections.push(fd),
                Err(rustix::io::Errno::AGAIN) => {
                    full = true;
                    break;
                }
                Err(error) => panic!("{error}"),
            }
        }
        assert!(full);
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_secs(1));
            drop(listener);
        });
        let start = Instant::now();
        assert!(forward(&socket, Screen::Study, start + Duration::from_millis(100)).is_err());
        let elapsed = start.elapsed();
        release.join().unwrap();
        assert!(elapsed < Duration::from_millis(500), "{elapsed:?}");
    }

    #[test]
    fn rejects_unsafe_runtime_directory_and_invalid_protocol() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("ipc");
        let db = temp.path().join("test.db");
        let instance = Instance::start_in(&db, Screen::Dashboard, &root)
            .unwrap()
            .unwrap();
        let mut stream = UnixStream::connect(&instance.socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream.write_all(&[255]).unwrap();
        let mut ack = [1];
        stream.read_exact(&mut ack).unwrap();
        assert_eq!(ack, [0]);
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Instance::start_in(&db, Screen::Study, &root).is_err());
    }
}
