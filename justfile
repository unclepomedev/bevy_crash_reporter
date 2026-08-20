default:
	@just --list

# code ================================================================================================================

fix-rs:
    cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features --workspace -- -D warnings

# fmt and clippy-fix
fmt-rs:
    just fix-rs
    cargo fmt --all
fmt: fmt-rs
f: fmt-rs

build-rs:
    cargo build --workspace --all-features
build: build-rs
b: build-rs

test-rs:
    cargo test --workspace --all-features
test: test-rs
t: test-rs

doc-rs:
    cargo doc --workspace --no-deps --all-features
doc: doc-rs
d: doc-rs

# local ci ============================================================================================================

check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features
c: check
