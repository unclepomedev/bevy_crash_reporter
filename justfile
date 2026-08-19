default:
	@just --list

# code ================================================================================================================

fix-rs:
    cargo clippy --fix --allow-dirty --allow-staged --all-targets --workspace -- -D warnings

# fmt and clippy-fix
fmt-rs:
    just fix-rs
    cargo fmt --all
fmt: fmt-rs
f: fmt-rs

build-rs:
    cargo build --workspace
build: build-rs
b: build-rs

test-rs:
    cargo test --workspace
test: test-rs
t: test-rs

doc-rs:
    cargo doc --workspace --no-deps
doc: doc-rs
d: doc-rs

# local ci ============================================================================================================

check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
c: check
