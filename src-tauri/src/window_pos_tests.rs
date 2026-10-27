//! テスト仕様書 6.4（window_pos.rs）。関数名の先頭はテストIDに対応する。
use super::*;

fn primary() -> Rect {
    Rect { x: 0, y: 0, width: 1920, height: 1080 }
}

/// 判定点（左上 + GRIP_OFFSET）が (x, y) になるウィンドウの位置
fn pos_for_grip(x: i32, y: i32) -> (i32, i32) {
    (x - GRIP_OFFSET.0, y - GRIP_OFFSET.1)
}

#[test]
fn wp01_first_run_always_repositions() {
    assert!(needs_reposition((100, 100), &[primary()], true));
}
#[test]
fn wp02_inside_monitor_keeps() {
    assert!(!needs_reposition((100, 100), &[primary()], false));
}
#[test]
fn wp03_outside_all_monitors() {
    assert!(needs_reposition((3000, 100), &[primary()], false));
    assert!(needs_reposition((100, -500), &[primary()], false));
}
#[test]
fn wp04_second_monitor_on_left() {
    let left = Rect { x: -1280, y: 0, width: 1280, height: 1024 };
    assert!(!needs_reposition((-1000, 100), &[primary(), left], false));
    assert!(needs_reposition((-1000, 100), &[primary()], false));
}
#[test]
fn wp05_right_edge_is_outside() {
    assert!(needs_reposition(pos_for_grip(1920, 500), &[primary()], false));
    assert!(!needs_reposition(pos_for_grip(1919, 500), &[primary()], false));
    assert!(needs_reposition(pos_for_grip(500, 1080), &[primary()], false));
}
#[test]
fn wp06_top_left_corner_is_inside() {
    assert!(!needs_reposition(pos_for_grip(0, 0), &[primary()], false));
    assert!(needs_reposition(pos_for_grip(-1, 0), &[primary()], false));
    assert!(needs_reposition(pos_for_grip(0, -1), &[primary()], false));
}
#[test]
fn wp07_no_monitors() {
    assert!(needs_reposition((100, 100), &[], false));
}
#[test]
fn tr01_scale_100() {
    assert_eq!(top_right(&primary(), 320, 1.0), (1920 - 320 - 16, 16));
}
#[test]
fn tr02_scale_150() {
    assert_eq!(top_right(&primary(), 480, 1.5), (1920 - 480 - 24, 24));
}
#[test]
fn tr03_monitor_offset() {
    let second = Rect { x: 1920, y: -200, width: 2560, height: 1440 };
    assert_eq!(top_right(&second, 320, 1.0), (1920 + 2560 - 320 - 16, -200 + 16));
}
