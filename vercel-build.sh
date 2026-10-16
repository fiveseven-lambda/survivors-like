rustup default nightly
rustup component add rust-src --toolchain nightly
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
cargo build --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir pkg target/wasm32-unknown-unknown/release/survivors_like.wasm
npm install
npm run build
