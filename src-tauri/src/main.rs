// 发布版不弹出控制台窗口（Windows）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tingyi_lib::run()
}
