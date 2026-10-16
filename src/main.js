import "./style.css";
import init from "/pkg/survivors_like.js";

const worker = new Worker(new URL("worker.js", import.meta.url), {
  type: "module",
});
const memory = new WebAssembly.Memory({
  initial: 256,
  maximum: 1024,
  shared: true,
});
worker.postMessage(memory);
init({ memory }).then((wasm) => {
  main.innerHTML =
    "<div><button id='fullscreen'>Fullscreen</button></div><canvas id='app'></canvas>";
  fullscreen.onclick = () => app.requestFullscreen();
  wasm.app();
});
