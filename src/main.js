import "./style.css";
import init from "/pkg/survivors_like.js";

if(navigator.gpu){
  init().then(wasm => {
    main.innerHTML =
      "<div><button id='fullscreen'>Fullscreen</button></div><canvas id='app'></canvas>";
    fullscreen.onclick = () => app.requestFullscreen();
    wasm.app();
  });
}else{
  main.innerHTML = "<p>Your device or browser doesn't support WebGPU, which is required to run this application.</p>";
}
