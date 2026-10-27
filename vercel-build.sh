rustup install stable
rustup target add wasm32-unknown-unknown
curl https://wasm-bindgen.github.io/wasm-pack/installer/init.sh -sSf | sh
wasm-pack build --target web
npm install
npm run build
