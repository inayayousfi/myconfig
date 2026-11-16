use typed_fs_rs::{EmbeddedDirectory, EmbeddedFile, embed_dir};

embed_dir!(pub static EMBEDDED_TREE = "tests/fixtures/tree");

#[test]
fn macro_declares_a_recursive_typed_static_directory() {
    let file: &EmbeddedFile = &EMBEDDED_TREE.read_me_txt;
    assert_eq!(file.content, b"root file\n");
    assert_eq!(file.path_from_root, "read-me.txt");
    let nested_file = &EMBEDDED_TREE.nested_dir.child_bin;
    assert_eq!(nested_file.content, b"child file\n");
    assert_eq!(nested_file.path_from_root, "nested-dir/child.bin");
    let paths: Vec<_> = EMBEDDED_TREE
        .files()
        .iter()
        .map(|file| file.path_from_root)
        .collect();
    assert_eq!(paths, ["nested-dir/child.bin", "read-me.txt"]);
}
