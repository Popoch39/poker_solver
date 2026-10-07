//! The bot targets the local client's window and nothing else.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use nitro_bot::{
    CaptureError, ClientWindow, Grabber, TargetError, Window, WindowList, capture_client,
    find_client_window, parse_hyprctl_clients,
};
use nitro_local_client::{APP_ID, Frame, TITLE, card_back};

fn window(title: &str, class: &str, stable_id: &str) -> Window {
    Window {
        address: format!("0x{stable_id}"),
        title: title.to_owned(),
        class: class.to_owned(),
        stable_id: Some(stable_id.to_owned()),
    }
}

fn client_window() -> Window {
    window(TITLE, APP_ID, "18000008")
}

/// A window list that gives each of `lists` in turn, then the last one.
struct Windows(RefCell<VecDeque<Vec<Window>>>);

impl Windows {
    fn new(lists: impl IntoIterator<Item = Vec<Window>>) -> Self {
        Self(RefCell::new(lists.into_iter().collect()))
    }
}

impl WindowList for Windows {
    fn windows(&self) -> Result<Vec<Window>, TargetError> {
        let mut lists = self.0.borrow_mut();
        if lists.len() > 1 {
            Ok(lists.pop_front().unwrap())
        } else {
            Ok(lists.front().cloned().unwrap_or_default())
        }
    }
}

fn desktop() -> Vec<Window> {
    vec![
        window("~/poker_solver", "Alacritty", "18000001"),
        window("Winamax", "winamax", "18000002"),
        window(
            "Nitro local client - Mozilla Firefox",
            "firefox",
            "18000003",
        ),
    ]
}

#[test]
fn targets_the_window_titled_like_the_local_client() {
    let mut windows = desktop();
    windows.insert(1, client_window());
    let target = find_client_window(&Windows::new([windows])).unwrap();
    assert_eq!(target.window(), &client_window());
}

#[test]
fn refuses_when_the_local_client_is_not_open() {
    assert_eq!(
        find_client_window(&Windows::new([desktop()])),
        Err(TargetError::NotFound)
    );
}

#[test]
fn refuses_a_window_titled_like_the_client_from_another_application() {
    let mut windows = desktop();
    windows.push(window(TITLE, "firefox", "18000004"));
    assert_eq!(
        find_client_window(&Windows::new([windows])),
        Err(TargetError::WrongApplication {
            class: "firefox".to_owned()
        })
    );
}

#[test]
fn refuses_when_a_look_alike_sits_next_to_the_client() {
    let mut windows = desktop();
    windows.push(client_window());
    windows.push(window(TITLE, "firefox", "18000004"));
    assert_eq!(
        find_client_window(&Windows::new([windows])),
        Err(TargetError::WrongApplication {
            class: "firefox".to_owned()
        })
    );
}

#[test]
fn refuses_to_choose_between_two_client_windows() {
    let mut windows = desktop();
    windows.push(client_window());
    windows.push(window(TITLE, APP_ID, "18000009"));
    assert_eq!(
        find_client_window(&Windows::new([windows])),
        Err(TargetError::Ambiguous(2))
    );
}

#[test]
fn refuses_a_client_window_it_cannot_name_to_the_compositor() {
    let mut windows = desktop();
    windows.push(Window {
        stable_id: None,
        ..client_window()
    });
    assert_eq!(
        find_client_window(&Windows::new([windows])),
        Err(TargetError::NoStableId)
    );
}

/// Records what it was asked to grab, and returns any image.
#[derive(Default)]
struct FakeGrabber {
    grabbed: RefCell<Vec<Window>>,
    calls: Cell<u32>,
}

impl Grabber for FakeGrabber {
    fn grab(&self, window: &ClientWindow) -> Result<Frame, CaptureError> {
        self.calls.set(self.calls.get() + 1);
        self.grabbed.borrow_mut().push(window.window().clone());
        Ok(card_back())
    }
}

#[test]
fn captures_only_the_local_clients_window() {
    let mut windows = desktop();
    windows.push(client_window());
    let grabber = FakeGrabber::default();
    capture_client(&Windows::new([windows]), &grabber).unwrap();
    assert_eq!(*grabber.grabbed.borrow(), [client_window()]);
}

#[test]
fn grabs_nothing_when_no_window_is_the_client() {
    let mut windows = desktop();
    windows.push(window(TITLE, "firefox", "18000004"));
    let grabber = FakeGrabber::default();
    let result = capture_client(&Windows::new([windows]), &grabber);
    assert!(matches!(result, Err(CaptureError::Target(_))), "{result:?}");
    assert_eq!(grabber.calls.get(), 0);
}

#[test]
fn drops_a_capture_if_the_client_window_changed_meanwhile() {
    let mut before = desktop();
    before.push(client_window());
    // The client closed during the capture, and another window took its
    // place with the same title.
    let mut after = desktop();
    after.push(window(TITLE, APP_ID, "1800000a"));
    let grabber = FakeGrabber::default();
    let result = capture_client(&Windows::new([before, after]), &grabber);
    assert_eq!(result, Err(CaptureError::WindowChanged));
}

#[test]
fn parses_hyprctls_client_list() {
    // Trimmed from `hyprctl clients -j` on Hyprland 0.56.
    let json = r#"[
      {
        "address": "0x63a6c1f0", "mapped": true, "hidden": false,
        "at": [40, 80], "size": [960, 640], "workspace": {"id": 2, "name": "2"},
        "floating": true, "monitor": 1, "class": "nitro-local-client",
        "title": "Nitro local client", "initialClass": "nitro-local-client",
        "initialTitle": "Nitro local client", "pid": 4242, "xwayland": false,
        "pinned": false, "fullscreen": 0, "grouped": [], "tags": [],
        "focusHistoryID": 0, "stableId": "18000008"
      },
      {
        "address": "0x63a6d000", "class": "Alacritty", "title": "~", "pid": 1
      }
    ]"#;
    assert_eq!(
        parse_hyprctl_clients(json).unwrap(),
        [
            Window {
                address: "0x63a6c1f0".to_owned(),
                title: TITLE.to_owned(),
                class: APP_ID.to_owned(),
                stable_id: Some("18000008".to_owned()),
            },
            Window {
                address: "0x63a6d000".to_owned(),
                title: "~".to_owned(),
                class: "Alacritty".to_owned(),
                stable_id: None,
            },
        ]
    );
}
