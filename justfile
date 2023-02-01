dev:
    cargo fmt
    cargo clippy
    cargo test --all-features
    cargo miri test --all-features

doc:
    RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc --open --no-deps --all-features
