// The Stamp panel. It keeps no state of its own: the mod sends the state after
// "hello", because the host loads the panel again when the theme changes.
"use strict";

const form = document.getElementById("form");
const text = document.getElementById("text");
const colors = document.getElementById("colors");
const status = document.getElementById("status");

function send(message) {
  parent.postMessage(JSON.stringify(message), "*");
}

function show(state) {
  text.value = state.text;
  for (const old of colors.querySelectorAll("label")) old.remove();
  for (const name of state.colors) {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "radio";
    input.name = "color";
    input.value = name;
    input.checked = name === state.color;
    label.append(input, " " + name);
    colors.append(label);
  }
  status.className = "";
  status.textContent = "Stamp: " + state.text + ", " + state.color + ".";
}

window.addEventListener("message", (event) => {
  if (event.source !== parent || typeof event.data !== "string") return;
  let message;
  try {
    message = JSON.parse(event.data);
  } catch {
    return;
  }
  if (message.kind === "state") show(message);
  if (message.kind === "error") {
    status.className = "error";
    status.textContent = message.message;
  }
});

for (const button of document.querySelectorAll("[data-text]")) {
  button.addEventListener("click", () => {
    text.value = button.dataset.text;
    text.focus();
  });
}

form.addEventListener("submit", (event) => {
  event.preventDefault();
  const checked = colors.querySelector("input:checked");
  send({ kind: "set", text: text.value, color: checked ? checked.value : "red" });
});

send({ kind: "hello" });
