use x11rb::connection::Connection;
use x11rb::properties::WmHints;
use x11rb::protocol::xproto::{self, AtomEnum, ConnectionExt as _, EventMask, Window};
use x11rb::protocol::shape;

/// X11 housekeeping for the overlay window: never takes focus (WM_HINTS
/// input=false), no taskbar entry, empty input shape, and always-on-top —
/// re-applied on every map and on a timer, since WMs reset these.
pub fn apply(title: &str) {
    let title = title.to_owned();
    std::thread::spawn(move || {
        let (conn, screen_num) = match x11rb::connect(None) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("crosshair: cannot open X connection: {e}");
                return;
            }
        };
        let root = conn.setup().roots[screen_num].root;

        let Some(atom_name) = intern(&conn, b"_NET_WM_NAME") else { return };
        let Some(atom_wm_name) = intern(&conn, b"WM_NAME") else { return };
        let Some(atom_state) = intern(&conn, b"_NET_WM_STATE") else { return };
        let Some(atom_above) = intern(&conn, b"_NET_WM_STATE_ABOVE") else { return };
        let Some(atom_skip_taskbar) = intern(&conn, b"_NET_WM_STATE_SKIP_TASKBAR") else { return };

        // The window may not be mapped yet — poll until we find it.
        let xid = loop {
            if let Some(x) = find_by_title(&conn, root, &[atom_name, atom_wm_name], &title) {
                break x;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        };

        apply_all(&conn, root, xid, atom_state, atom_above, atom_skip_taskbar);

        // Re-apply every 2 s: WMs reset input shapes, WM_HINTS and stacking
        // state (Hyprland's XWM re-asserts FULLSCREEN+FOCUSED over a one-shot
        // ABOVE on fullscreen XWayland windows).
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            apply_all(&conn, root, xid, atom_state, atom_above, atom_skip_taskbar);
        }
    });
}

fn apply_all<C: Connection>(
    conn: &C,
    root: Window,
    xid: Window,
    atom_state: u32,
    atom_above: u32,
    atom_skip_taskbar: u32,
) {
    set_input_shape(conn, xid);
    set_input_hints(conn, xid);
    set_state(conn, root, xid, atom_state, atom_above);
    set_state(conn, root, xid, atom_state, atom_skip_taskbar);
    raise(conn, xid);
}

fn intern<C: Connection>(conn: &C, name: &[u8]) -> Option<u32> {
    conn.intern_atom(false, name).ok()?.reply().ok().map(|r| r.atom)
}

fn find_by_title<C: Connection>(conn: &C, win: Window, atoms: &[u32], want: &str) -> Option<Window> {
    let tree = conn.query_tree(win).ok()?.reply().ok()?;
    for &child in &tree.children {
        for &atom in atoms {
            if let Some(t) = window_title(conn, child, atom) {
                if t == want {
                    return Some(child);
                }
            }
        }
        if let Some(found) = find_by_title(conn, child, atoms, want) {
            return Some(found);
        }
    }
    None
}

fn window_title<C: Connection>(conn: &C, win: Window, atom: u32) -> Option<String> {
    let cookie = conn
        .get_property(false, win, atom, AtomEnum::ANY, 0, 1024)
        .ok()?;
    let reply = cookie.reply().ok()?;
    if reply.format != 8 || reply.value.is_empty() {
        return None;
    }
    String::from_utf8(reply.value).ok()
}

fn set_input_shape<C: Connection>(conn: &C, xid: Window) {
    // Empty XShape input region → clicks pass through. An empty rectangle list
    // is the XFIXES-free way (XWayland on this machine rejects
    // xfixes::create_region with BadRequest).
    let result = shape::rectangles(
        conn,
        shape::SO::SET,
        shape::SK::INPUT,
        xproto::ClipOrdering::UNSORTED,
        xid,
        0,
        0,
        &[],
    );
    match result {
        Ok(cookie) => {
            if let Err(e) = cookie.check() {
                eprintln!("crosshair: x11 input shape failed: {e}");
            }
        }
        Err(e) => eprintln!("crosshair: x11 input shape send failed: {e}"),
    }
}

/// WM_HINTS input=false: the WM must never give this window focus.
fn set_input_hints<C: Connection>(conn: &C, xid: Window) {
    let mut hints = WmHints::new();
    hints.input = Some(false);
    match hints.set(conn, xid) {
        Ok(cookie) => {
            if let Err(e) = cookie.check() {
                eprintln!("crosshair: x11 wm_hints failed: {e}");
            }
        }
        Err(e) => eprintln!("crosshair: x11 wm_hints send failed: {e}"),
    }
}

/// Send a `_NET_WM_STATE` ClientMessage with action 1 (add) for `state`.
fn set_state<C: Connection>(conn: &C, root: Window, xid: Window, atom_state: u32, state: u32) {
    let event = xproto::ClientMessageEvent {
        response_type: xproto::CLIENT_MESSAGE_EVENT,
        format: 32,
        sequence: 0,
        window: xid,
        type_: atom_state,
        data: xproto::ClientMessageData::from([1, state, 0, 0, 0]),
    };
    let result = xproto::send_event(
        conn,
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    );
    match result {
        Ok(cookie) => {
            if let Err(e) = cookie.check() {
                eprintln!("crosshair: x11 {state:#x} message failed: {e}");
            }
        }
        Err(e) => eprintln!("crosshair: x11 {state:#x} message send failed: {e}"),
    }
}

/// Restack directly above, in addition to the ABOVE state message.
fn raise<C: Connection>(conn: &C, xid: Window) {
    let aux = xproto::ConfigureWindowAux::new().stack_mode(xproto::StackMode::ABOVE);
    let result = xproto::configure_window(conn, xid, &aux);
    match result {
        Ok(cookie) => {
            if let Err(e) = cookie.check() {
                eprintln!("crosshair: x11 restack failed: {e}");
            }
        }
        Err(e) => eprintln!("crosshair: x11 restack send failed: {e}"),
    }
}
