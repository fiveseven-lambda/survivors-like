import "./style.css";
import init from "/pkg/survivors_like.js";

init().then(wasm => {
  main.innerHTML =
    "<div><button id='fullscreen'>Fullscreen</button></div><canvas id='app'></canvas>";
  fullscreen.onclick = () => app.requestFullscreen();
  wasm.app();
});
