//! Omaspec addition to winit 0.30.13: file drag-and-drop on Wayland.
//!
//! `wl_data_device` delivers a drop of `text/uri-list`. Each `file://` path
//! becomes `WindowEvent::DroppedFile`, which is how an SVG or PNG opens its
//! diagram source. The bytes are read when the pipe is readable, after the
//! event loop has flushed the receive request. Reading it inside the drop
//! event would wait on a writer that has not been told yet.
//!
//! Hyprland sends the clipboard (`selection`) only to the first data device
//! a client makes, and that is this one, made before the clipboard's own.
//! The offer is kept for the clipboard (vendored `smithay-clipboard`),
//! which reads it through `platform::wayland::selection_offer`.

use std::io::{self, ErrorKind, Read};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use sctk::compositor::SurfaceData;
use sctk::data_device_manager::data_device::{DataDeviceData, DataDeviceHandler};
use sctk::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use sctk::data_device_manager::data_source::DataSourceHandler;
use sctk::data_device_manager::{ReadPipe, WritePipe};
use sctk::reexports::calloop::{self, PostAction};
use sctk::reexports::client::protocol::wl_data_device::WlDataDevice;
use sctk::reexports::client::protocol::wl_data_device_manager::DndAction;
use sctk::reexports::client::protocol::wl_data_offer::WlDataOffer;
use sctk::reexports::client::protocol::wl_data_source::WlDataSource;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{Connection, Proxy, QueueHandle};

use crate::event::WindowEvent;
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, WindowId};

/// A dropped list longer than this is ignored.
const MAX_LIST: usize = 1024 * 1024;

/// The clipboard's current offer and its MIME types, `None` when it is empty.
static SELECTION: Mutex<Option<(WlDataOffer, Vec<String>)>> = Mutex::new(None);

/// The clipboard's current offer and its MIME types.
pub(crate) fn selection_offer() -> Option<(WlDataOffer, Vec<String>)> {
    SELECTION.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

impl DataDeviceHandler for WinitState {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        data_device: &WlDataDevice,
        _x: f64,
        _y: f64,
        surface: &WlSurface,
    ) {
        self.dnd_window = None;
        let Some(offer) = drag_offer(data_device) else {
            return;
        };
        if !accept_uri_list(&offer) {
            return;
        }
        let window = window_of(surface);
        if self.windows.get_mut().contains_key(&window) {
            self.dnd_window = Some(window);
        }
    }

    fn leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _data_device: &WlDataDevice) {
        self.dnd_window = None;
    }

    fn motion(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _data_device: &WlDataDevice,
        _x: f64,
        _y: f64,
    ) {
        // The paths arrive with the drop, not with each move.
    }

    fn selection(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        data_device: &WlDataDevice,
    ) {
        let offer = data_device
            .data::<DataDeviceData>()
            .and_then(DataDeviceData::selection_offer)
            .map(|offer| (offer.inner().clone(), offer.with_mime_types(<[String]>::to_vec)));
        *SELECTION.lock().unwrap_or_else(PoisonError::into_inner) = offer;
    }

    fn drop_performed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        data_device: &WlDataDevice,
    ) {
        let Some(window) = self.dnd_window.take() else {
            return;
        };
        let Some(offer) = drag_offer(data_device) else {
            return;
        };
        let Some(mime) = uri_list_mime(&offer) else {
            offer.finish();
            return;
        };
        let pipe = match offer.receive(mime) {
            Ok(pipe) => pipe,
            Err(err) => {
                tracing::warn!("file drop: {err}");
                offer.finish();
                return;
            }
        };
        set_nonblocking(&pipe);
        let mut got = Vec::new();
        let on_error = offer.clone();
        let inserted =
            self.loop_handle
                .insert_source(
                    pipe,
                    move |(), file, state: &mut WinitState| match read_list(file, &mut got) {
                        ReadList::More => PostAction::Continue,
                        ReadList::Done => {
                            deliver(state, window, &got);
                            offer.finish();
                            PostAction::Remove
                        }
                        ReadList::Failed(err) => {
                            tracing::warn!("file drop: {err}");
                            offer.finish();
                            PostAction::Remove
                        }
                    },
                );
        if let Err(err) = inserted {
            tracing::warn!("file drop: {err:?}");
            on_error.finish();
        }
    }
}

impl DataOfferHandler for WinitState {
    fn source_actions(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        offer: &mut DragOffer,
        _actions: DndAction,
    ) {
        accept_uri_list(offer);
    }

    fn selected_action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _offer: &mut DragOffer,
        _action: DndAction,
    ) {
        // `set_actions` already asked for a copy. The compositor's choice
        // is what the drop uses.
    }
}

impl DataSourceHandler for WinitState {
    // Omaspec only receives drops. It never offers a data source.
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: String,
        _: WritePipe,
    ) {
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

sctk::delegate_data_device!(WinitState);

fn drag_offer(device: &WlDataDevice) -> Option<DragOffer> {
    device
        .data::<DataDeviceData>()
        .and_then(DataDeviceData::drag_offer)
}

/// Accept `text/uri-list` as a copy, or reject the offer. Says whether it
/// was accepted.
fn accept_uri_list(offer: &DragOffer) -> bool {
    match uri_list_mime(offer) {
        Some(mime) => {
            offer.accept_mime_type(offer.serial, Some(mime));
            offer.set_actions(DndAction::Copy, DndAction::Copy);
            true
        }
        None => {
            offer.accept_mime_type(offer.serial, None);
            false
        }
    }
}

fn uri_list_mime(offer: &DragOffer) -> Option<String> {
    offer.with_mime_types(|types| types.iter().find(|mime| is_uri_list(mime)).cloned())
}

fn is_uri_list(mime: &str) -> bool {
    mime.split(';')
        .next()
        .unwrap_or(mime)
        .trim()
        .eq_ignore_ascii_case("text/uri-list")
}

/// The window surface, walking up from a subsurface the way the pinch does.
fn window_of(surface: &WlSurface) -> WindowId {
    let parent = match surface.data::<SurfaceData>() {
        Some(data) => data.parent_surface().unwrap_or(surface),
        None => surface,
    };
    wayland::make_wid(parent)
}

fn set_nonblocking(pipe: &ReadPipe) {
    let fd = pipe.as_raw_fd();
    // Safety: `fd` is the live read end of this pipe. The calls only change
    // its status flags and do not close it.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags != -1 {
            let _ = libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
}

enum ReadList {
    More,
    Done,
    Failed(io::Error),
}

fn read_list(file: &mut calloop::generic::NoIoDrop<std::fs::File>, got: &mut Vec<u8>) -> ReadList {
    let mut buf = [0u8; 8192];
    loop {
        match read_pipe(file, &mut buf) {
            Ok(0) => return ReadList::Done,
            Ok(n) if got.len().saturating_add(n) > MAX_LIST => {
                return ReadList::Failed(io::Error::new(
                    ErrorKind::InvalidData,
                    "dropped file list is larger than 1 MB",
                ));
            }
            Ok(n) => got.extend_from_slice(&buf[..n]),
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) if err.kind() == ErrorKind::WouldBlock => return ReadList::More,
            Err(err) => return ReadList::Failed(err),
        }
    }
}

fn read_pipe(
    file: &mut calloop::generic::NoIoDrop<std::fs::File>,
    buf: &mut [u8],
) -> io::Result<usize> {
    // Safety: this reads the pipe and does not drop or replace it. calloop
    // requires the file to stay registered until the source is removed.
    unsafe { file.get_mut() }.read(buf)
}

fn deliver(state: &mut WinitState, window: WindowId, bytes: &[u8]) {
    let paths = paths_from_uri_list(bytes);
    if paths.is_empty() {
        tracing::warn!("file drop had no local file");
    }
    for path in paths {
        state
            .events_sink
            .push_window_event(WindowEvent::DroppedFile(path), window);
    }
    state.dispatched_events = true;
}

/// `text/uri-list`: one `file://` URI per line, percent-encoded, CRLF
/// separated. A `#` line is a comment. A host other than `localhost` is
/// skipped.
fn paths_from_uri_list(bytes: &[u8]) -> Vec<PathBuf> {
    let text = String::from_utf8_lossy(bytes);
    let mut paths = Vec::new();
    for line in text.trim_end_matches('\0').split('\n') {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(decoded) = percent_decode(line) else {
            continue;
        };
        if let Some(path) = file_uri(&decoded) {
            paths.push(path);
        }
    }
    paths
}

fn file_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file:")?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let path = if let Some(path) = rest.strip_prefix('/') {
        path
    } else {
        let (host, path) = rest.split_once('/')?;
        if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
            return None;
        }
        path
    };
    let path = Path::new(path);
    (!path.as_os_str().is_empty()).then(|| PathBuf::from("/").join(path))
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = hex(*bytes.get(i + 1)?)?;
            let lo = hex(*bytes.get(i + 2)?)?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_list_becomes_local_paths() {
        let raw = "\
file:///tmp/a%20b.svg\r\n\
# comment\r\n\
file://localhost/tmp/c.png\r\n\
file://other/tmp/d.svg\r\n\
\r\n";
        assert_eq!(
            paths_from_uri_list(raw.as_bytes()),
            vec![PathBuf::from("/tmp/a b.svg"), PathBuf::from("/tmp/c.png")]
        );
    }
}
