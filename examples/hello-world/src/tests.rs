use super::*;

#[test]
fn metadata_uses_cargo_defaults_and_manifest_overrides() {
    let metadata = HelloWorld::METADATA;
    assert_eq!(metadata.name, c"Rust Hello World");
    assert_eq!(metadata.log_tag, c"RUSTHELLO");
    assert_eq!(
        metadata.version.to_bytes(),
        env!("CARGO_PKG_VERSION").as_bytes()
    );
    assert_eq!(
        metadata.author.to_bytes(),
        env!("CARGO_PKG_AUTHORS").as_bytes()
    );
    assert_eq!(
        metadata.url.to_bytes(),
        env!("CARGO_PKG_REPOSITORY").as_bytes()
    );
}
