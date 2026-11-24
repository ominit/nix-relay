use super::*;

#[test]
fn frames_round_trip() {
    let frame = encode_frame(42, b"nar bytes");
    assert_eq!(decode_frame(&frame), Some((42, &b"nar bytes"[..])));
}

#[test]
fn short_frames_are_rejected() {
    assert_eq!(decode_frame(&[0, 1, 2]), None);
}

#[test]
fn store_path_names_drop_hash_and_drv_suffix() {
    assert_eq!(
        store_path_name("/nix/store/647c6ibvpl6wi04whphg1whl2z0nvwy4-hello-2.12.3.drv"),
        "hello-2.12.3"
    );
    assert_eq!(
        store_path_name("/nix/store/kdfjzy4rnqm7mmw0w8a46rsamflw5lfx-b"),
        "b"
    );
}

#[test]
fn messages_use_tagged_json() {
    let json = serde_json::to_string(&ClientMessage::UploadDone).unwrap();
    assert_eq!(json, r#"{"type":"upload_done"}"#);
}
