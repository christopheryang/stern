default: dev

dev:
    cd desktop/stern-tauri && cargo tauri dev

build:
    cd desktop/stern-tauri && cargo tauri build

lint:
    cargo clippy --workspace -- -D warnings

clean:
    cargo clean --manifest-path desktop/stern-tauri/Cargo.toml
    rm -rf desktop/stern-app/dist
