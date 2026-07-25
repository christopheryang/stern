default: dev

dev:
    cd desktop/stern-tauri && cargo tauri dev

build:
    cd desktop/stern-tauri && cargo tauri build

lint:
    cargo clippy --workspace -- -D warnings

fmt:
    cargo fmt --all -- --check

fmt-fix:
    cargo fmt --all

test-core:
    cargo test --manifest-path core/stern-core/Cargo.toml
    cargo test --manifest-path core/stern-ipc/Cargo.toml

test-desktop:
    cargo test --manifest-path desktop/stern-tauri/Cargo.toml

test: test-core test-desktop

clean:
    cargo clean --manifest-path desktop/stern-tauri/Cargo.toml
    rm -rf desktop/stern-app/dist
