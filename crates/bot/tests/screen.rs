//! The screen a click may land on: every monitor of the compositor's
//! layout, in logical pixels.

use nitro_bot::{ScreenArea, ScreenPoint, parse_hyprctl_monitors};

#[test]
fn the_screen_spans_every_monitor_at_its_logical_size() {
    // Trimmed from `hyprctl monitors -j` on Hyprland 0.56: a 4K monitor at
    // scale 2 left of a 1080p one at scale 1, and a portrait one, rotated,
    // above it.
    let json = r#"[
      {"id": 0, "name": "DP-1", "width": 3840, "height": 2160, "x": -1920,
       "y": 0, "scale": 2.00, "transform": 0, "disabled": false},
      {"id": 1, "name": "HDMI-A-1", "width": 1920, "height": 1080, "x": 0,
       "y": 0, "scale": 1.00, "transform": 0, "disabled": false},
      {"id": 2, "name": "DP-2", "width": 1920, "height": 1080, "x": 0,
       "y": -1920, "scale": 1.00, "transform": 1, "disabled": false}
    ]"#;
    let area = parse_hyprctl_monitors(json).unwrap();
    assert_eq!(
        area,
        ScreenArea {
            x: -1920,
            y: -1920,
            width: 3840,
            height: 3000,
        }
    );
    assert!(area.contains(ScreenPoint { x: -1920, y: 0 }));
    assert!(area.contains(ScreenPoint { x: 1919, y: 1079 }));
    assert!(!area.contains(ScreenPoint { x: 1920, y: 500 }));
    assert!(!area.contains(ScreenPoint { x: 0, y: 1080 }));
}

#[test]
fn disabled_monitors_are_no_part_of_the_screen() {
    let json = r#"[
      {"width": 1920, "height": 1080, "x": 0, "y": 0, "scale": 1.0,
       "transform": 0, "disabled": false},
      {"width": 1920, "height": 1080, "x": 1920, "y": 0, "scale": 1.0,
       "transform": 0, "disabled": true}
    ]"#;
    let area = parse_hyprctl_monitors(json).unwrap();
    assert_eq!((area.width, area.height), (1920, 1080));
}

#[test]
fn no_monitor_is_no_screen() {
    assert!(parse_hyprctl_monitors("[]").is_err());
}
