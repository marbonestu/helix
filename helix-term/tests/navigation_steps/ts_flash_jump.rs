use std::io::Write as _;

use cucumber::given;

use super::NavigationWorld;

/// Stage a Rust-language buffer by writing the content to a temporary `.rs`
/// file. Helix detects the language from the extension and activates
/// treesitter, which is required for ts-flash jump to find nodes.
///
/// The cursor starts at position 0 (the beginning of the file) by default,
/// matching how helix opens a fresh file.
#[given(regex = r#"^the Rust buffer contains "(.+)"$"#)]
fn given_rust_buffer_contains(world: &mut NavigationWorld, content: String) {
    let text = content.replace("\\n", "\n");

    let mut temp = tempfile::Builder::new()
        .suffix(".rs")
        .tempfile()
        .expect("failed to create temp .rs file");
    temp.write_all(text.as_bytes())
        .expect("failed to write temp .rs file");

    world.rust_temp_file = Some(temp);
}
