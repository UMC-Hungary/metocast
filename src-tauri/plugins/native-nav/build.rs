const COMMANDS: &[&str] = &["install", "set_active"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();
}
