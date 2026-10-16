import init from "/pkg/survivors_like.js";

onmessage = (event) => {
  init({ memory: event.data }).then((wasm) => {
    wasm.generate_field();
  });
};
