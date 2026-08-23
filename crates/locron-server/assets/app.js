// Viewer bootstrap: detect the session cookie and show the paste form when absent.
// The access token is submitted once as a POST body and never appears in a URL.
"use strict";

const pastePanel = document.getElementById("paste-panel");
const appPanel = document.getElementById("app-panel");
const pasteForm = document.getElementById("paste-form");
const pasteError = document.getElementById("paste-error");

async function sessionStatus() {
  const response = await fetch("/api/v1/session");
  if (response.ok) return true;
  if (response.status === 401) return false;
  throw new Error(`session check failed: ${response.status}`);
}

async function submitToken(event) {
  event.preventDefault();
  pasteError.hidden = true;
  const token = document.getElementById("token").value;
  const response = await fetch("/api/v1/session", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ token }),
  });
  if (response.ok) {
    location.reload();
  } else {
    pasteError.textContent = "The access token was rejected.";
    pasteError.hidden = false;
  }
}

pasteForm.addEventListener("submit", submitToken);

sessionStatus()
  .then((authenticated) => {
    pastePanel.hidden = authenticated;
    appPanel.hidden = !authenticated;
  })
  .catch(() => {
    pasteError.textContent = "The dashboard is unreachable.";
    pasteError.hidden = false;
  });
