curl https://wasm-bindgen.github.io/wasm-pack/installer/init.sh -sSf | sh -s -- -y
wasm-pack build --target web
npm run build
