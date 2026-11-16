// The embed macro lists folders that Cargo does not track, so a file added under
// `dotfiles/` must rebuild this crate to be embedded.
fn main() {
    println!("cargo:rerun-if-changed=../../dotfiles");
}
