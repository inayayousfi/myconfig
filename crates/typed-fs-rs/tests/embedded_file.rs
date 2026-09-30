use typed_fs_rs::{EmbeddedDirectory, EmbeddedFile, embed_dir};

embed_dir!(pub static EMBEDDED_TREE = "crates/typed-fs-rs/tests/fixtures/tree");

#[test]
fn macro_produces_recursive_typed_directory() {
    let tree = embed_dir!("crates/typed-fs-rs/tests/fixtures/tree");

    let _: &dyn EmbeddedDirectory = &tree;
    let file: &EmbeddedFile = &tree.read_me_txt;
    assert_eq!(file.content, b"root file\n");
    assert_eq!(file.path_from_root, "read-me.txt");
    let nested_file = &tree.nested_dir.child_bin;
    assert_eq!(nested_file.content, b"child file\n");
    assert_eq!(nested_file.path_from_root, "nested-dir/child.bin");
    assert_eq!(tree.files().len(), 2);
    assert!(
        tree.files()
            .iter()
            .any(|file| file.path_from_root == "nested-dir/child.bin")
    );
}

#[test]
fn macro_declares_a_global_static_directory() {
    let _: &dyn EmbeddedDirectory = &EMBEDDED_TREE;
    assert_eq!(EMBEDDED_TREE.read_me_txt.content, b"root file\n");
    assert_eq!(
        EMBEDDED_TREE.nested_dir.child_bin.path_from_root,
        "nested-dir/child.bin"
    );
}

#[test]
fn macro_preserves_colliding_file_paths_and_assigns_distinct_fields() {
    let files = embed_dir!("crates/typed-fs-rs/tests/fixtures/colliding-names");

    assert_eq!(files.size_bdiag.path_from_root, "size_bdiag");
    assert_eq!(files.size_dash_bdiag.path_from_root, "size_dash_bdiag");
    assert_eq!(files.size_dash_bdiag_2.path_from_root, "size-bdiag");
    assert_eq!(files.size_dash_bdiag_2.content, b"dash\n");
}

#[test]
fn embedded_file_writes_nested_path_and_verifies_its_content() {
    let root = std::env::temp_dir().join(format!(
        "typed-fs-rs-embedded-file-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let file = EmbeddedFile {
        content: b"expected content\n",
        path_from_root: "nested/output.txt",
        executable: false,
    };

    file.write(&root).unwrap();
    file.verify(&root).unwrap();
    assert_eq!(
        std::fs::read(root.join("nested/output.txt")).unwrap(),
        b"expected content\n"
    );

    std::fs::write(root.join("nested/output.txt"), b"wrong content\n").unwrap();
    assert_eq!(
        file.verify(&root).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    std::fs::remove_dir_all(root).unwrap();
}
