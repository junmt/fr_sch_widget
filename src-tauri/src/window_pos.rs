//! ウィジェットを置く位置の計算（純粋な関数）

/// モニターの範囲（物理ピクセル）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.width as i32 && y >= self.y && y < self.y + self.height as i32
    }
}

/// ウィンドウの左上からこれだけ内側（ヘッダーのあたり）がモニターに入っていれば、ドラッグして動かせる
pub const GRIP_OFFSET: (i32, i32) = (40, 16);
/// 右上に置くときの画面の端からの余白（論理ピクセル）
pub const MARGIN: f64 = 16.0;

/// 位置を直す必要があるか（初回の起動、または保存した位置がどのモニターにも入っていない）
pub fn needs_reposition(window_pos: (i32, i32), monitors: &[Rect], first_run: bool) -> bool {
    let (x, y) = (window_pos.0 + GRIP_OFFSET.0, window_pos.1 + GRIP_OFFSET.1);
    first_run || !monitors.iter().any(|m| m.contains(x, y))
}

/// モニターの右上に置くときのウィンドウの左上の座標
pub fn top_right(monitor: &Rect, window_width: u32, scale_factor: f64) -> (i32, i32) {
    let margin = (MARGIN * scale_factor) as i32;
    (monitor.x + monitor.width as i32 - window_width as i32 - margin, monitor.y + margin)
}

#[cfg(test)]
#[path = "window_pos_tests.rs"]
mod tests;
