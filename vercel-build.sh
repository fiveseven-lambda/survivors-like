curl https://wasm-bindgen.github.io/wasm-pack/installer/init.sh -sSf | sh
wasm-pack build --target web
npm install
npm run build
