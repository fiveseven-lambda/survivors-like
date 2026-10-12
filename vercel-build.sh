rustup default 1.97.1
rustup target add wasm32-unknown-unknown
curl https://wasm-bindgen.github.io/wasm-pack/installer/init.sh -sSf | sh -s -- -y
wasm-pack build --target web
npm run build
