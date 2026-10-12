curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
curl https://wasm-bindgen.github.io/wasm-pack/installer/init.sh -sSf | sh -s -- -y
wasm-pack build --target web
npm run build
