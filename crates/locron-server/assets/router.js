// Hash router: parse location.hash and dispatch to the registered view
// renderer. Hash routing survives refresh and bookmarking with zero server
// fallbacks (MDN `hashchange`).
"use strict";

const Router = (() => {
  const routes = [];

  function register(pattern, render) {
    // A pattern is a literal path with optional `:param` segments.
    routes.push({ pattern, render });
  }

  function match(path) {
    for (const route of routes) {
      const pattern = route.pattern.split("/");
      const segments = path.split("/");
      if (pattern.length !== segments.length) continue;
      const params = {};
      let ok = true;
      for (let i = 0; i < pattern.length; i += 1) {
        if (pattern[i].startsWith(":")) {
          params[pattern[i].slice(1)] = decodeURIComponent(segments[i]);
        } else if (pattern[i] !== segments[i]) {
          ok = false;
          break;
        }
      }
      if (ok) return { render: route.render, params };
    }
    return null;
  }

  function currentPath() {
    const hash = location.hash || "#/jobs";
    return hash.startsWith("#") ? hash.slice(1) : hash;
  }

  function dispatch() {
    const view = document.getElementById("view");
    const path = currentPath();
    for (const link of document.querySelectorAll("#topbar nav a[data-nav]")) {
      const root = link.dataset.nav;
      const current = path === root || path.startsWith(`${root}/`);
      if (current) link.setAttribute("aria-current", "page");
      else link.removeAttribute("aria-current");
    }
    const match = Router.match(path);
    if (!match) {
      view.innerHTML = Components.emptyState("Unknown route", `"${path}" is not a dashboard view.`);
      return;
    }
    match.render(view, match.params);
    document.getElementById("main-content").focus({ preventScroll: true });
  }

  function start() {
    window.addEventListener("hashchange", dispatch);
    dispatch();
  }

  function navigate(path) {
    if (currentPath() === path) {
      dispatch();
    } else {
      location.hash = path;
    }
  }

  return { register, match, start, navigate, currentPath };
})();
